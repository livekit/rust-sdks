// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests for the typed room surface.
//!
//! Every wait runs on [`block_on`], a plain thread with no tokio context. A
//! foreign executor drives a UniFFI future the same way, so a wait or a
//! command that needs an ambient runtime fails here.
//!
//! The tests with `#[ignore]` need a local `livekit-server --dev`. They read
//! `LK_TEST_URL`, `LK_TEST_API_KEY` and `LK_TEST_API_SECRET`, the names that
//! the other server tests of this crate use. The defaults are the values of
//! the development server.

use std::{
    collections::HashMap,
    future::Future,
    net::TcpListener,
    pin::pin,
    sync::{Arc, Weak},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use livekit_token::{AccessToken, VideoGrants};

use super::{events::EventQueue, *};

const LIMIT: Duration = Duration::from_secs(20);

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Polls `future` on this thread. Returns `None` when `limit` passes first.
fn poll_for<T>(limit: Duration, future: impl Future<Output = T>) -> Option<T> {
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    let deadline = Instant::now() + limit;
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return Some(value);
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        thread::park_timeout(deadline - now);
    }
}

#[track_caller]
fn block_on<T>(future: impl Future<Output = T>) -> T {
    poll_for(LIMIT, future).expect("the wait did not end in time")
}

impl FfiRoom {
    async fn state_changed(&self, after: u64) -> Result<FfiRoomState, NativeSessionError> {
        wait_for(&self.shared.changed, || {
            let inner = self.shared.inner.lock();
            if inner.revision > after {
                Some(Ok(inner.snapshot()))
            } else if inner.terminal == Some(NativeSessionError::Cancelled) || inner.finished {
                inner.terminal.clone().map(Err)
            } else {
                None
            }
        })
        .await
    }
}

fn limits(max_items: u32, max_bytes: u64) -> NativeEventCursorOptions {
    NativeEventCursorOptions { max_items, max_bytes }
}

fn cursor_options() -> NativeEventCursorOptions {
    limits(4096, 16 * 1024 * 1024)
}

fn connect_options() -> Arc<FfiRoomOptions> {
    Arc::new(
        FfiRoomOptions::new().with_join_retries(0).with_connect_timeout(Duration::from_secs(2)),
    )
}

fn update(revision: u64, event: NativeRoomEvent) -> NativeRoomUpdate {
    NativeRoomUpdate { revision, event }
}

fn token_event(token: &str) -> NativeRoomEvent {
    NativeRoomEvent::TokenRefreshed { token: token.to_owned() }
}

fn room_event(metadata: &str) -> NativeRoomEvent {
    NativeRoomEvent::RoomUpdated {
        room: RoomInfo {
            sid: None,
            state: ConnectionState::Connected,
            lossy_dc_options: Default::default(),
            reliable_dc_options: Default::default(),
            name: "room".to_owned(),
            metadata: metadata.to_owned(),
            num_participants: 0,
            num_publishers: 0,
            max_participants: 0,
            active_recording: false,
            empty_timeout: 0,
            departure_timeout: 0,
            creation_time: 0,
        },
    }
}

fn data_event(kind: DataPacketKind, size: usize) -> NativeRoomEvent {
    NativeRoomEvent::DataReceived {
        packet: NativeDataPacket {
            payload: vec![0; size],
            topic: None,
            kind,
            participant_identity: None,
        },
    }
}

fn cursor_over(queue: &Arc<EventQueue>) -> Arc<NativeRoomEventCursor> {
    NativeRoomEventCursor::new(queue.clone(), FfiRoom::new().state())
}

fn next(cursor: &NativeRoomEventCursor) -> Result<Option<NativeRoomEvent>, NativeSessionError> {
    block_on(cursor.next()).map(|update| update.map(|update| update.event))
}

#[test]
fn item_overflow_fails_the_cursor() {
    let queue = Arc::new(EventQueue::new(limits(2, 1024)));
    let cursor = cursor_over(&queue);
    for name in ["a", "b", "c"] {
        queue.push(update(1, token_event(name)));
    }

    // The failure replaces the backlog, so the consumer cannot read a
    // sequence with a hole in it.
    let overflow = NativeSessionError::Overflow { items: 2, bytes: 0 };
    assert_eq!(next(&cursor), Err(overflow.clone()));
    queue.push(update(2, token_event("d")));
    assert_eq!(next(&cursor), Err(overflow.clone()));
    assert!(!native_session_error_is_shutdown(overflow));
}

#[test]
fn byte_overflow_fails_for_reliable_and_drops_lossy() {
    let queue = Arc::new(EventQueue::new(limits(100, 10)));
    let cursor = cursor_over(&queue);
    queue.push(update(1, data_event(DataPacketKind::Reliable, 8)));

    queue.push(update(1, data_event(DataPacketKind::Lossy, 8)));
    assert_eq!(cursor.dropped_lossy_packets(), 1);
    assert_eq!(next(&cursor), Ok(Some(data_event(DataPacketKind::Reliable, 8))));

    // The read released the bytes.
    queue.push(update(1, data_event(DataPacketKind::Reliable, 8)));
    queue.push(update(1, data_event(DataPacketKind::Reliable, 8)));
    assert_eq!(next(&cursor), Err(NativeSessionError::Overflow { items: 1, bytes: 8 }));
}

#[test]
fn coalesce_keeps_the_newest_value_and_the_order() {
    let queue = Arc::new(EventQueue::new(limits(3, 1024)));
    let cursor = cursor_over(&queue);
    queue.push(update(1, room_event("one")));
    queue.push(update(2, room_event("two")));
    queue.push(update(2, token_event("ordered")));
    // An ordered event is between the two values, so this one is not merged.
    queue.push(update(3, room_event("three")));
    queue.push(update(4, room_event("four")));
    queue.finish();

    assert_eq!(block_on(cursor.next()), Ok(Some(update(2, room_event("two")))));
    assert_eq!(next(&cursor), Ok(Some(token_event("ordered"))));
    assert_eq!(block_on(cursor.next()), Ok(Some(update(4, room_event("four")))));
    assert_eq!(next(&cursor), Ok(None));
    assert_eq!(next(&cursor), Ok(None));
}

#[test]
fn coalesce_never_moves_a_revision_back() {
    let queue = Arc::new(EventQueue::new(limits(8, 1024)));
    let cursor = cursor_over(&queue);
    let speakers = NativeRoomEvent::ActiveSpeakersChanged { speakers: Vec::new() };
    queue.push(update(1, room_event("old")));
    queue.push(update(2, speakers.clone()));
    // Regression: the newer room value took the first position and the
    // consumer read revision 3 before revision 2.
    queue.push(update(3, room_event("new")));
    queue.finish();

    assert_eq!(block_on(cursor.next()), Ok(Some(update(2, speakers))));
    assert_eq!(block_on(cursor.next()), Ok(Some(update(3, room_event("new")))));
    assert_eq!(next(&cursor), Ok(None));
}

#[test]
fn cancel_wakes_a_pending_next() {
    let queue = Arc::new(EventQueue::new(cursor_options()));
    let cursor = cursor_over(&queue);
    let waiter = {
        let cursor = cursor.clone();
        thread::spawn(move || next(&cursor))
    };
    thread::sleep(Duration::from_millis(50));

    let started = Instant::now();
    cursor.cancel();
    cursor.cancel();
    assert_eq!(waiter.join().expect("waiter"), Err(NativeSessionError::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(1));

    queue.push(update(1, token_event("late")));
    assert_eq!(next(&cursor), Err(NativeSessionError::Cancelled));
}

#[test]
fn cancel_before_connect_is_terminal() {
    let session = FfiRoom::new();
    let cursor = session.events(cursor_options());
    assert_eq!(cursor.start_state().connection, NativeConnectionState::Idle);
    let before = session.state();

    session.cancel();
    session.cancel();

    let cancelled = Err(NativeSessionError::Cancelled);
    assert_eq!(next(&cursor), cancelled);
    assert_eq!(
        block_on(session.connect("ws://127.0.0.1:1".to_owned(), String::new(), connect_options())),
        Err(NativeSessionError::Cancelled)
    );
    assert_eq!(
        block_on(session.publish_data(
            vec![1],
            NativeDataPublishOptions {
                reliable: true,
                topic: None,
                destination_identities: Vec::new()
            }
        )),
        Err(NativeSessionError::Cancelled)
    );

    // The cancel is a change, so a waiter sees the last snapshot one time.
    let last = block_on(session.state_changed(before.revision)).expect("last snapshot");
    assert_eq!(last.connection, NativeConnectionState::Disconnected);
    assert_eq!(block_on(session.state_changed(last.revision)), Err(NativeSessionError::Cancelled));

    // Regression: a session that never connected has no task to finish it.
    block_on(session.disconnect());
    assert_eq!(next(&session.events(cursor_options())), cancelled);
}

#[test]
fn disconnect_before_connect_ends_the_cursor() {
    let session = FfiRoom::new();
    let cursor = session.events(cursor_options());
    block_on(session.disconnect());
    block_on(session.disconnect());

    assert_eq!(
        next(&cursor),
        Ok(Some(NativeRoomEvent::Disconnected { reason: DisconnectReason::ClientInitiated }))
    );
    assert_eq!(next(&cursor), Ok(None));
    assert_eq!(
        block_on(session.connect("ws://127.0.0.1:1".to_owned(), String::new(), connect_options())),
        Err(NativeSessionError::Closed)
    );
    assert_eq!(block_on(session.state_changed(u64::MAX)), Err(NativeSessionError::Closed));
}

#[test]
fn connect_failure_reports_the_cause_and_finishes() {
    // A port with no listener: the connection is refused at once.
    let port = TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port();
    let session = FfiRoom::new();
    let cursor = session.events(cursor_options());

    let result =
        block_on(session.connect(format!("ws://127.0.0.1:{port}"), "t".into(), connect_options()));
    assert!(
        matches!(
            result,
            Err(NativeSessionError::Connection { .. } | NativeSessionError::Signal { .. })
        ),
        "{result:?}"
    );

    assert_eq!(
        next(&cursor),
        Ok(Some(NativeRoomEvent::Disconnected { reason: DisconnectReason::JoinFailure }))
    );
    assert_eq!(next(&cursor), Ok(None));
    block_on(session.disconnect());
    let state = session.state();
    assert_eq!(state.connection, NativeConnectionState::Disconnected);
    assert_eq!(state.disconnect_reason, Some(DisconnectReason::JoinFailure));

    let shared = Arc::downgrade(&session.shared);
    drop((session, cursor));
    assert!(shared.upgrade().is_none(), "the session leaked");
}

/// The check of the `room-apis-host-transport` build: with no host client
/// registered, connect fails at once with the typed cause. A bundled network
/// stack in the build makes this fail, because connect then reaches the
/// closed port and reports a transport error instead.
///
/// No test in this binary registers a client. The livekit-net registry is
/// process-wide and its first registration stays for the whole process, so a
/// test that registers one must run in a different binary.
#[cfg(feature = "room-apis-host-transport")]
#[test]
fn connect_without_host_transport_is_not_configured() {
    use super::errors::NativeSignalFailure;

    assert!(!livekit_net::has_ws_client() && !livekit_net::has_http_client());
    let session = FfiRoom::new();

    let result =
        block_on(session.connect("ws://127.0.0.1:1".to_owned(), "t".into(), connect_options()));
    assert_eq!(
        result,
        Err(NativeSessionError::Signal { failure: NativeSignalFailure::TransportNotConfigured })
    );
    block_on(session.disconnect());
}

// Only with the bundled stack: this test needs a handshake that stays pending
// on a real socket. The host-transport build has no client in this binary, so
// its connect fails at once and can end before the cancel.
#[cfg(feature = "room-apis")]
#[test]
fn cancel_during_connect_returns_at_once() {
    // The listener accepts the TCP connection in its backlog and never
    // answers, so the signal handshake waits for its timeout.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let session = FfiRoom::new();
    let cursor = session.events(cursor_options());

    let connect = {
        let session = session.clone();
        thread::spawn(move || {
            let url = format!("ws://127.0.0.1:{port}");
            block_on(session.connect(url, "t".into(), connect_options()))
        })
    };
    let connecting = block_on(session.state_changed(0)).expect("connecting");
    assert_eq!(connecting.connection, NativeConnectionState::Connecting);
    let state_waiter = {
        let session = session.clone();
        thread::spawn(move || block_on(session.state_changed(connecting.revision)))
    };
    thread::sleep(Duration::from_millis(100));

    let started = Instant::now();
    session.cancel();
    assert_eq!(connect.join().expect("connect"), Err(NativeSessionError::Cancelled));
    assert_eq!(next(&cursor), Err(NativeSessionError::Cancelled));
    let last = state_waiter.join().expect("state waiter").expect("last snapshot");
    assert_eq!(last.connection, NativeConnectionState::Disconnected);
    assert!(started.elapsed() < Duration::from_secs(1), "cancel waited for the connect");

    // `disconnect` waits for the connect attempt that the core still runs.
    block_on(session.disconnect());
    let shared = Arc::downgrade(&session.shared);
    drop((session, cursor));
    assert!(shared.upgrade().is_none(), "the session leaked");
    drop(listener);
}

#[test]
fn repeated_create_and_release_leaves_nothing() {
    let mut released: Vec<Weak<Shared>> = Vec::new();
    for round in 0..60 {
        let session = FfiRoom::new();
        let cursor = session.events(cursor_options());
        released.push(Arc::downgrade(&session.shared));
        match round % 3 {
            0 => session.cancel(),
            1 => block_on(session.disconnect()),
            // To release the handles has the same effect as a cancel.
            _ => {}
        }
        drop(session);
        assert!(next(&cursor).is_err() || round % 3 == 1);
    }
    assert!(released.iter().all(|shared| shared.upgrade().is_none()));
}

#[test]
fn log_forward_delivers_records_of_the_selected_level() {
    use super::log_forward::{log_forward_bootstrap, log_forward_receive};

    log_forward_bootstrap(log::LevelFilter::Info);
    let marker = format!("typed log probe {}", std::process::id());
    log::debug!("{marker} below the level");
    log::info!("{marker}");

    // Other tests log too, so read until the probe arrives.
    let deadline = Instant::now() + LIMIT;
    loop {
        assert!(Instant::now() < deadline, "the probe did not arrive");
        let entry = block_on(log_forward_receive()).expect("the log never ends");
        assert!(!entry.message.ends_with("below the level"));
        if entry.message == marker {
            assert_eq!(entry.level, log::Level::Info);
            break;
        }
    }
}

// The tests below need a server.

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn unique(name: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .subsec_nanos();
    format!("typed-{name}-{}-{nanos}", std::process::id())
}

struct Peer {
    identity: String,
    session: Arc<FfiRoom>,
    cursor: Arc<NativeRoomEventCursor>,
    /// Membership at connect; its participant and publication handles remain live.
    first: FfiRoomState,
    /// The revision of the last event that this peer read.
    revision: u64,
    seen: Vec<NativeRoomEvent>,
}

impl Peer {
    fn join(room: &str, identity: &str) -> Self {
        Self::join_with(room, identity, cursor_options())
    }

    fn join_with(room: &str, identity: &str, cursor: NativeEventCursorOptions) -> Self {
        Self::join_options(
            room,
            identity,
            cursor,
            Arc::new(connect_options().with_connect_timeout(Duration::from_secs(10))),
        )
    }

    fn join_options(
        room: &str,
        identity: &str,
        cursor: NativeEventCursorOptions,
        options: Arc<FfiRoomOptions>,
    ) -> Self {
        let token = AccessToken::with_api_key(
            &env_or("LK_TEST_API_KEY", "devkey"),
            &env_or("LK_TEST_API_SECRET", "secret"),
        )
        .with_identity(identity)
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_owned(),
            can_update_own_metadata: Some(true),
            ..Default::default()
        })
        .to_jwt()
        .expect("token");

        let session = FfiRoom::new();
        let cursor = session.events(cursor);
        let url = env_or("LK_TEST_URL", "ws://127.0.0.1:7880");
        let first = block_on(session.connect(url, token, options)).expect("connect");

        assert_eq!(first.connection, NativeConnectionState::Connected);
        assert_eq!(first.room.as_ref().map(|room| room.name.as_str()), Some(room));
        let local = first.local_participant.as_ref().expect("local participant");
        assert_eq!(local.identity(), identity);
        assert!(local.is_local());
        Self {
            identity: identity.to_owned(),
            session,
            cursor,
            revision: first.revision,
            first,
            seen: Vec::new(),
        }
    }

    /// Reads events until `pick` accepts one.
    fn expect<T>(&mut self, mut pick: impl FnMut(&NativeRoomEvent) -> Option<T>) -> T {
        loop {
            let update = block_on(self.cursor.next())
                .expect("the cursor failed")
                .expect("the cursor ended before the expected event");
            // Initial order: no event is older than the snapshot of `connect`,
            // and the revisions never go back.
            assert!(update.revision >= self.revision, "{update:?} after {}", self.revision);
            self.revision = update.revision;
            self.seen.push(update.event);
            if let Some(found) = pick(self.seen.last().expect("pushed")) {
                return found;
            }
        }
    }

    fn expect_data(&mut self) -> NativeDataPacket {
        self.expect(|event| match event {
            NativeRoomEvent::DataReceived { packet } => Some(packet.clone()),
            _ => None,
        })
    }

    fn send(&self, payload: &[u8], reliable: bool, topic: Option<&str>) {
        let options = NativeDataPublishOptions {
            reliable,
            topic: topic.map(str::to_owned),
            destination_identities: Vec::new(),
        };
        block_on(self.session.publish_data(payload.to_vec(), options)).expect("publish data");
    }

    /// Disconnects, reads the cursor to its end and checks that nothing of
    /// the session stays alive. Returns every event that the peer read.
    fn leave(mut self) -> Vec<NativeRoomEvent> {
        block_on(self.session.disconnect());
        let reason = DisconnectReason::ClientInitiated;
        while let Some(update) = block_on(self.cursor.next()).expect("the cursor failed") {
            self.seen.push(update.event);
        }
        assert_eq!(self.seen.last(), Some(&NativeRoomEvent::Disconnected { reason }));

        let state = self.session.state();
        assert_eq!(state.connection, NativeConnectionState::Disconnected);
        assert_eq!(state.disconnect_reason, Some(reason));
        assert!(state.remote_participants.is_empty() && state.publications.is_empty());
        // The first snapshot is a value. The session did not change it.
        assert_eq!(self.first.connection, NativeConnectionState::Connected);

        let shared = Arc::downgrade(&self.session.shared);
        drop((self.session, self.cursor));
        assert!(shared.upgrade().is_none(), "{} leaked its session", self.identity);
        self.seen
    }
}

fn remote_identities(state: &FfiRoomState) -> Vec<String> {
    let mut identities: Vec<_> = state.remote_participants.iter().map(|p| p.identity()).collect();
    identities.sort_unstable();
    identities
}

fn payloads(events: &[NativeRoomEvent]) -> Vec<&[u8]> {
    events
        .iter()
        .filter_map(|event| match event {
            NativeRoomEvent::DataReceived { packet } => Some(packet.payload.as_slice()),
            _ => None,
        })
        .collect()
}

// Serial: under load, peer connections that start together can pass the ICE
// limit of the core.
#[serial_test::serial]
#[test]
#[ignore = "needs a local livekit-server"]
fn two_rooms_exchange_state_and_data() {
    let (room_a, room_b) = (unique("a"), unique("b"));

    // Room A and room B are live in this process at the same time.
    let mut alice = Peer::join(&room_a, "alice");
    assert!(alice.first.remote_participants.is_empty());
    let mut carol = Peer::join(&room_b, "carol");
    let mut bob = Peer::join(&room_a, "bob");
    let mut dave = Peer::join(&room_b, "dave");

    // Initial order: a participant that was present is in the first snapshot
    // and has no event. A participant that joins later has an event.
    assert_eq!(remote_identities(&bob.first), ["alice"]);
    assert_eq!(remote_identities(&dave.first), ["carol"]);
    let joined = |event: &NativeRoomEvent| match event {
        NativeRoomEvent::ParticipantConnected { participant } => Some(participant.clone()),
        _ => None,
    };
    let seen_bob = alice.expect(joined);
    assert_eq!((seen_bob.identity().as_str(), seen_bob.is_local()), ("bob", false));
    assert!(alice.revision > alice.first.revision);
    assert_eq!(carol.expect(joined).identity(), "dave");

    // Reliable data, in order, with topic and sender.
    for index in 0..50u8 {
        alice.send(&[b'a', index], true, Some("topic-a"));
    }
    for index in 0..50u8 {
        let packet = bob.expect_data();
        assert_eq!(packet.payload, [b'a', index]);
        assert_eq!(packet.kind, DataPacketKind::Reliable);
        assert_eq!(packet.topic.as_deref(), Some("topic-a"));
        assert_eq!(packet.participant_identity.as_deref(), Some("alice"));
    }
    carol.send(b"room-b", true, None);
    let packet = dave.expect_data();
    assert_eq!((packet.payload.as_slice(), packet.topic), (b"room-b".as_slice(), None));
    assert_eq!(packet.participant_identity.as_deref(), Some("carol"));

    // Lossy data in the other direction. The channel can drop a packet, so
    // send until one arrives.
    let lossy = {
        let sender = bob.session.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let sending = thread::spawn(move || {
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                let options = NativeDataPublishOptions {
                    reliable: false,
                    topic: None,
                    destination_identities: vec!["alice".to_owned()],
                };
                block_on(sender.publish_data(b"lossy".to_vec(), options)).expect("publish lossy");
                thread::sleep(Duration::from_millis(100));
            }
        });
        let packet = alice.expect_data();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        sending.join().expect("lossy sender");
        packet
    };
    assert_eq!(lossy.payload, b"lossy");
    assert_eq!(lossy.kind, DataPacketKind::Lossy);
    assert_eq!(lossy.participant_identity.as_deref(), Some("bob"));

    // Participant state: the event carries the new value, the old snapshot
    // keeps the old value, and the revision wait returns the new snapshot.
    let attributes = HashMap::from([("mood".to_owned(), "calm".to_owned())]);
    block_on(alice.session.set_attributes(attributes.clone())).expect("set attributes");
    block_on(alice.session.set_name("Alice A".to_owned())).expect("set name");
    let (changed, participant) = bob.expect(|event| match event {
        NativeRoomEvent::ParticipantAttributesChanged { participant, changed_attributes } => {
            Some((changed_attributes.clone(), participant.clone()))
        }
        _ => None,
    });
    assert_eq!((changed, participant.identity().as_str()), (attributes.clone(), "alice"));
    assert_eq!(participant.attributes(), attributes);
    bob.expect(|event| match event {
        NativeRoomEvent::ParticipantNameChanged { participant, .. } => {
            (participant.name() == "Alice A").then_some(())
        }
        _ => None,
    });
    assert_eq!(
        bob.first.remote_participants[0].attributes(),
        attributes,
        "retained handles read current core values"
    );
    let newer = block_on(bob.session.state_changed(bob.first.revision)).expect("newer state");
    assert!(newer.revision > bob.first.revision);
    let current = bob.session.state();
    assert_eq!(current.remote_participants[0].attributes(), attributes);
    assert_eq!(current.remote_participants[0].name(), "Alice A");

    // Room A ends for alice. Bob sees it, and room B keeps working.
    let alice_events = alice.leave();
    let left = bob.expect(|event| match event {
        NativeRoomEvent::ParticipantDisconnected { participant } => Some(participant.clone()),
        _ => None,
    });
    assert_eq!(left.identity(), "alice");
    assert!(bob.session.state().remote_participants.is_empty());
    dave.send(b"still-b", true, None);
    assert_eq!(carol.expect_data().payload, b"still-b");

    // No event crossed from one room to the other.
    let (bob_events, carol_events, dave_events) = (bob.leave(), carol.leave(), dave.leave());
    assert!(payloads(&alice_events).iter().all(|payload| *payload == b"lossy"));
    assert_eq!(payloads(&bob_events).len(), 50);
    assert_eq!(payloads(&carol_events), [b"still-b"]);
    assert_eq!(payloads(&dave_events), [b"room-b"]);
    for events in [&bob_events, &dave_events] {
        assert!(
            !events.iter().any(|e| matches!(e, NativeRoomEvent::ParticipantConnected { .. })),
            "a participant of the first snapshot also had an event"
        );
    }
}

// Serial: under load, peer connections that start together can pass the ICE
// limit of the core.
#[serial_test::serial]
#[test]
#[ignore = "needs a local livekit-server"]
fn slow_consumer_overflows_and_starts_again() {
    let room = unique("overflow");
    let alice = Peer::join(&room, "alice");
    let mut bob = Peer::join_with(&room, "bob", limits(8, 1024));

    for index in 0..40u8 {
        alice.send(&[index], true, None);
    }
    // Bob reads nothing, so the 9th packet is over the limit. A read would
    // release room in the backlog, so look at the backlog and do not read.
    let deadline = Instant::now() + LIMIT;
    while bob.cursor.is_open() {
        assert!(Instant::now() < deadline, "the cursor did not overflow");
        thread::sleep(Duration::from_millis(20));
    }
    let error = block_on(bob.cursor.next()).expect_err("the backlog failed");
    assert!(matches!(error, NativeSessionError::Overflow { .. }), "{error:?}");
    assert_eq!(block_on(bob.cursor.next()), Err(error));

    // The session did not end. A new cursor starts from its own snapshot.
    assert_eq!(bob.session.state().connection, NativeConnectionState::Connected);
    bob.cursor = bob.session.events(cursor_options());
    let start = bob.cursor.start_state();
    assert_eq!(remote_identities(&start), ["alice"]);
    bob.revision = start.revision;
    // Packets of the first burst can still be on the way, so wait for the marker.
    alice.send(b"after", true, None);
    bob.expect(|event| match event {
        NativeRoomEvent::DataReceived { packet } => (packet.payload == b"after").then_some(()),
        _ => None,
    });

    alice.leave();
    bob.leave();
}

// Serial: under load, peer connections that start together can pass the ICE
// limit of the core.
#[serial_test::serial]
#[test]
#[ignore = "needs a local livekit-server"]
fn cancel_races_a_real_connect() {
    let room = unique("race");
    let observer = Peer::join(&room, "observer");

    for round in 0..6u64 {
        let victim = FfiRoom::new();
        let cursor = victim.events(cursor_options());
        let connect = {
            let victim = victim.clone();
            let token = AccessToken::with_api_key(
                &env_or("LK_TEST_API_KEY", "devkey"),
                &env_or("LK_TEST_API_SECRET", "secret"),
            )
            .with_identity(&format!("victim-{round}"))
            .with_grants(VideoGrants { room_join: true, room: room.clone(), ..Default::default() })
            .to_jwt()
            .expect("token");
            thread::spawn(move || {
                let url = env_or("LK_TEST_URL", "ws://127.0.0.1:7880");
                block_on(victim.connect(url, token, connect_options()))
            })
        };
        // Cancel at a different point of the connection in each round.
        thread::sleep(Duration::from_millis(round * 40));
        victim.cancel();

        match connect.join().expect("connect") {
            // The connection completed before the cancel.
            Ok(state) => assert_eq!(state.connection, NativeConnectionState::Connected),
            Err(error) => assert_eq!(error, NativeSessionError::Cancelled),
        }
        assert_eq!(victim.state().connection, NativeConnectionState::Disconnected);
        // The join of the session task: a room that connected late is closed here.
        block_on(victim.disconnect());
        let shared = Arc::downgrade(&victim.shared);
        drop((victim, cursor));
        assert!(shared.upgrade().is_none(), "round {round} leaked its session");
    }

    // Every room that the cancelled sessions opened is closed, so the server
    // reports no other participant.
    let mut state = observer.session.state();
    let deadline = Instant::now() + LIMIT;
    while !state.remote_participants.is_empty() {
        assert!(Instant::now() < deadline, "left behind: {:?}", remote_identities(&state));
        if let Some(newer) =
            poll_for(Duration::from_millis(500), observer.session.state_changed(state.revision))
        {
            state = newer.expect("observer state");
        }
    }
    observer.leave();
}

#[test]
fn cancel_supersedes_closed_during_stalled_teardown() {
    let session = FfiRoom::new();
    // Deterministic driver state: a close is pending and has not marked finished.
    session.shared.update(|inner| inner.connection = NativeConnectionState::Connecting);
    session.shared.terminate(NativeSessionError::Closed);
    let mut wait = Box::pin(session.state_changed(session.state().revision));
    assert!(poll_for(Duration::from_millis(10), &mut wait).is_none());
    session.cancel();
    let frozen = session.state();
    let _ = block_on(wait);
    assert_eq!(
        block_on(session.state_changed(frozen.revision)),
        Err(NativeSessionError::Cancelled)
    );
    session.shared.apply(RoomEvent::TokenRefreshed { token: "late".into() });
    session.shared.apply(RoomEvent::ConnectionStateChanged(ConnectionState::Connected));
    session.shared.update_sid("RM_late".to_owned().try_into().unwrap());
    assert_eq!(session.state(), frozen);
    session.shared.update(Inner::finish);
    block_on(session.disconnect());
}

#[test]
fn dropping_foreign_future_releases_core_work() {
    struct Resource(std::sync::mpsc::Sender<()>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.send(()).expect("destruction observed");
        }
    }
    let (done, released) = std::sync::mpsc::channel();
    let resource = Arc::new(Resource(done));
    let weak = Arc::downgrade(&resource);
    let mut call = Box::pin(on_runtime(async move {
        let _resource = resource;
        std::future::pending::<()>().await;
    }));
    assert!(poll_for(Duration::from_millis(20), &mut call).is_none());
    drop(call);
    released.recv_timeout(LIMIT).expect("runtime hop released the core future");
    assert!(weak.upgrade().is_none());
}

#[test]
fn foreign_future_runs_off_the_calling_executor() {
    let caller = std::thread::current().id();
    block_on(on_runtime(async move {
        assert_ne!(std::thread::current().id(), caller);
        tokio::task::yield_now().await;
        assert!(tokio::runtime::Handle::try_current().is_ok());
        tokio::time::timeout(Duration::from_secs(1), async {}).await.unwrap();
    }));
}

fn encrypted_options(key: &str) -> Arc<FfiRoomOptions> {
    use super::encryption::{NativeEncryptionOptions, NativeKeyProvider, NativeKeyProviderOptions};
    Arc::new(
        connect_options().with_encryption(Some(NativeEncryptionOptions {
            key_provider: NativeKeyProvider::new(
                NativeKeyProviderOptions::default(),
                Some(bytes::Bytes::copy_from_slice(key.as_bytes())),
            )
            .unwrap(),
            legacy_media_only: false,
        })),
    )
}

#[serial_test::serial]
#[test]
#[ignore = "requires a local LiveKit server with data tracks and participant data blobs enabled"]
fn real_room_helpers_streams_tracks_schema_and_rpc() {
    use crate::data_stream::common::{StreamByteOptions, StreamTextOptions};
    use crate::data_track::{
        common::{DataTrackFrame, DataTrackSchemaId},
        local::DataTrackOptions,
    };
    use livekit_datatrack::api::{DataTrackFrameEncoding, DataTrackSchemaEncoding};
    for encrypted in [false, true] {
        let room = unique("helpers");
        let options = || if encrypted { encrypted_options(&room) } else { connect_options() };
        let sender = Peer::join_options(&room, "sender", cursor_options(), options());
        let mut receiver = Peer::join_options(&room, "receiver", cursor_options(), options());
        assert_eq!(sender.session.encryption_status().unwrap().data_enabled, encrypted);
        assert_eq!(receiver.session.encryption_status().unwrap().data_enabled, encrypted);
        block_on(sender.session.publish_data(
            vec![0, 1, 255],
            NativeDataPublishOptions {
                reliable: true,
                topic: Some("encrypted-proof".into()),
                destination_identities: vec![],
            },
        ))
        .unwrap();
        receiver.expect(|event| match event {
            NativeRoomEvent::DataReceived { packet }
                if packet.topic.as_deref() == Some("encrypted-proof") =>
            {
                assert_eq!(packet.payload, vec![0, 1, 255]);
                Some(())
            }
            _ => None,
        });

        let info =
            block_on(sender.session.send_text("hello λ".into(), StreamTextOptions::default()))
                .expect("text");
        let reader = receiver.expect(|event| match event {
            NativeRoomEvent::TextStreamOpened { reader, identity } if identity == "sender" => {
                Some(reader.clone())
            }
            _ => None,
        });
        assert_eq!(reader.info().id, info.id);
        assert_eq!(
            reader.info().encryption_type,
            if encrypted {
                crate::data_stream::common::EncryptionType::Gcm
            } else {
                crate::data_stream::common::EncryptionType::None
            }
        );
        assert_eq!(block_on(reader.read_all()).expect("read text"), "hello λ");
        let bytes = bytes::Bytes::from_static(b"\x00\xffbytes");
        block_on(sender.session.send_bytes(bytes.clone(), StreamByteOptions::default()))
            .expect("bytes");
        let reader = receiver.expect(|event| match event {
            NativeRoomEvent::ByteStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        assert_eq!(block_on(reader.read_all()).expect("read bytes"), bytes);
        let path = std::env::temp_dir().join(unique("stream-file"));
        std::fs::write(&path, b"file payload").expect("temporary file");
        block_on(
            sender.session.send_file(path.to_string_lossy().into(), StreamByteOptions::default()),
        )
        .expect("send file");
        std::fs::remove_file(path).expect("remove file");
        let reader = receiver.expect(|event| match event {
            NativeRoomEvent::ByteStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        assert_eq!(block_on(reader.read_all()).expect("read file").as_ref(), b"file payload");
        let writer = block_on(sender.session.stream_text(StreamTextOptions::default()))
            .expect("text writer");
        block_on(writer.write("incremental".into())).expect("text write");
        block_on(writer.close(None, HashMap::from([("done".into(), "yes".into())])))
            .expect("text close");
        let reader = receiver.expect(|event| match event {
            NativeRoomEvent::TextStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        assert_eq!(block_on(reader.read_all()).expect("incremental text"), "incremental");
        assert_eq!(reader.info().attributes.get("done").map(String::as_str), Some("yes"));
        let writer = block_on(sender.session.stream_bytes(StreamByteOptions::default()))
            .expect("byte writer");
        block_on(writer.write(bytes.clone())).expect("byte write");
        let reader = receiver.expect(|event| match event {
            NativeRoomEvent::ByteStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        assert_eq!(block_on(reader.next()).expect("byte chunk"), Some(bytes));
        reader.close();
        assert!(block_on(reader.next()).is_err());
        block_on(writer.close(None, HashMap::new())).expect("byte close");

        // Completion is independent of chunk consumption, including overlapping streams.
        let first = block_on(sender.session.stream_text(StreamTextOptions::default())).unwrap();
        let first_reader = receiver.expect(|event| match event {
            NativeRoomEvent::TextStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        let second = block_on(sender.session.stream_text(StreamTextOptions::default())).unwrap();
        let second_reader = receiver.expect(|event| match event {
            NativeRoomEvent::TextStreamOpened { reader, .. } => Some(reader.clone()),
            _ => None,
        });
        block_on(first.write("first".into())).unwrap();
        block_on(second.write("second".into())).unwrap();
        block_on(second.close(None, HashMap::new())).unwrap();
        block_on(second_reader.wait_closed());
        assert!(poll_for(Duration::from_millis(100), first_reader.wait_closed()).is_none());
        block_on(first.close(None, HashMap::new())).unwrap();
        block_on(first_reader.wait_closed());
        assert_eq!(block_on(first_reader.read_all()).unwrap(), "first");
        assert_eq!(block_on(second_reader.read_all()).unwrap(), "second");

        let schema = || DataTrackSchemaId {
            name: "helper-schema".into(),
            encoding: DataTrackSchemaEncoding::JsonSchema,
        };
        block_on(sender.session.define_schema(schema(), "{}".into())).expect("define schema");
        assert_eq!(
            block_on(receiver.session.get_schema(schema(), "sender".into())).expect("get schema"),
            "{}"
        );
        let local = block_on(sender.session.publish_data_track(DataTrackOptions {
            name: "helper-track".into(),
            schema: Some(schema()),
            frame_encoding: Some(DataTrackFrameEncoding::Json),
        }))
        .expect("publish data track");
        let remote = receiver.expect(|event| match event {
            NativeRoomEvent::DataTrackPublished { track, identity } if identity == "sender" => {
                Some(track.clone())
            }
            _ => None,
        });
        assert_eq!(remote.info().uses_e2ee, encrypted);
        assert_eq!(receiver.session.state().data_tracks.len(), 1);
        let recovered = receiver
            .session
            .remote_data_track(remote.info().sid.to_string())
            .expect("snapshot handle");
        assert!(Arc::ptr_eq(&remote, &recovered));
        let stream = block_on(remote.subscribe()).expect("subscribe");
        local
            .try_push(DataTrackFrame {
                payload: bytes::Bytes::from_static(b"{}"),
                user_timestamp: Some(42),
            })
            .expect("push frame");
        let frame = block_on(stream.next()).expect("frame");
        assert_eq!(frame.payload.as_ref(), b"{}");
        assert_eq!(frame.user_timestamp, Some(42));
        local.unpublish();
        block_on(local.wait_for_unpublish());
        block_on(remote.wait_for_unpublish());
        assert!(block_on(stream.next()).is_none());

        block_on(receiver.session.register_rpc_method("echo".into())).expect("register RPC");
        for reply in [true, false] {
            let session = sender.session.clone();
            let call = thread::spawn(move || {
                block_on(session.perform_rpc(NativeRpcOptions {
                    destination_identity: "receiver".into(),
                    method: "echo".into(),
                    payload: "rpc payload".into(),
                    response_timeout_ms: if reply { 10_000 } else { 1_000 },
                    max_round_trip_latency_ms: 100,
                }))
            });
            let invocation = receiver.expect(|event| match event {
                NativeRoomEvent::RpcRequested { invocation } => Some(invocation.clone()),
                _ => None,
            });
            assert_eq!(invocation.info().payload, "rpc payload");
            if reply {
                assert!(invocation.respond("answer".into()));
            }
            let result = call.join().expect("RPC caller");
            if reply {
                assert_eq!(result.expect("RPC response"), "answer");
            } else {
                assert!(result.is_err());
                thread::sleep(Duration::from_millis(250));
            }
            assert!(!invocation.respond("late".into()));
        }
        block_on(receiver.session.unregister_rpc_method("echo".into())).expect("unregister");
        block_on(receiver.session.register_rpc_method("pending".into())).expect("register pending");
        let session = sender.session.clone();
        let pending = thread::spawn(move || {
            block_on(session.perform_rpc(NativeRpcOptions {
                destination_identity: "receiver".into(),
                method: "pending".into(),
                payload: String::new(),
                response_timeout_ms: 10_000,
                max_round_trip_latency_ms: 100,
            }))
        });
        let invocation = receiver.expect(|event| match event {
            NativeRoomEvent::RpcRequested { invocation } => Some(invocation.clone()),
            _ => None,
        });
        block_on(receiver.session.disconnect());
        assert!(pending.join().expect("pending caller").is_err());
        assert!(!invocation.respond("after disconnect".into()));

        let held_room = sender.session.shared.room().expect("connected room");
        let weak_room = Arc::downgrade(&held_room);
        let mut command = Box::pin(on_runtime(async move {
            let _held = held_room;
            std::future::pending::<()>().await;
        }));
        assert!(poll_for(Duration::from_millis(20), &mut command).is_none());
        drop(command);
        sender.leave();
        assert!(weak_room.upgrade().is_none(), "command retained actual room");
        receiver.leave();
    }
}

fn media_publish_options(source: TrackSource) -> TrackPublishOptions {
    TrackPublishOptions {
        source,
        dtx: false,
        red: true,
        simulcast: false,
        stream: String::new(),
        ..Default::default()
    }
}

#[test]
fn media_validation_and_child_handles_do_not_retain_session() {
    let session = FfiRoom::new();
    assert!(block_on(session.create_audio_track("bad".into(), 0)).is_err());
    assert!(block_on(session.create_video_track("bad".into(), u32::MAX, 180, false)).is_err());
    let audio = block_on(session.create_audio_track("local".into(), 1)).expect("source");
    block_on(audio.set_muted(true)).expect("mute before publishing");
    assert!(audio.info().unwrap().is_muted());
    block_on(audio.set_muted(false)).expect("unmute before publishing");
    assert!(!audio.info().unwrap().is_muted());
    assert!(block_on(audio.capture_frame(NativeAudioFrame {
        samples: vec![],
        sample_rate: 48_000,
        num_channels: 1,
        samples_per_channel: 480,
    }))
    .is_err());
    session.cancel();
    assert!(matches!(
        block_on(session.create_audio_track("closed".into(), 1)),
        Err(NativeSessionError::Cancelled)
    ));
    assert!(matches!(
        block_on(session.create_video_track("closed".into(), 640, 480, false)),
        Err(NativeSessionError::Cancelled)
    ));
    // This must reject before touching PlatformAudio or selecting a device.
    assert!(matches!(
        block_on(session.create_microphone_track("closed".into(), Some("invalid-device".into()))),
        Err(NativeSessionError::Cancelled)
    ));
    let weak = Arc::downgrade(&session.shared);
    block_on(session.disconnect());
    assert!(audio.info().is_err());
    drop(session);
    assert!(weak.upgrade().is_none(), "child audio handle retained session");
}

#[serial_test::serial]
#[test]
#[ignore = "requires a local LiveKit server; synthetic sources prove codec transport, not physical devices"]
fn real_room_media_initial_publications_audio_video_and_teardown() {
    for encrypted in [false, true] {
        let name = unique("media");
        let options = || if encrypted { encrypted_options(&name) } else { connect_options() };
        let sender = Peer::join_options(&name, "publisher", cursor_options(), options());
        let audio =
            block_on(sender.session.create_audio_track("pcm".into(), 1)).expect("PCM source");
        let publication = block_on(audio.publish(media_publish_options(TrackSource::Microphone)))
            .expect("publish audio");
        let info = publication.info().expect("audio publication");
        assert_eq!(
            info.encryption_type(),
            if encrypted { EncryptionType::Gcm } else { EncryptionType::None }
        );
        let sid = info.sid();
        let mut receiver = Peer::join_options(
            &name,
            "subscriber",
            cursor_options(),
            Arc::new(
                options().with_auto_subscribe(false).with_connect_timeout(Duration::from_secs(10)),
            ),
        );
        assert!(
            receiver
                .first
                .publications
                .iter()
                .any(|p| p.sid() == sid && p.participant_identity() == "publisher"),
            "initial publication missing"
        );
        let remote_publication = receiver
            .session
            .publication("publisher".into(), sid.clone())
            .expect("publication lookup");
        block_on(remote_publication.set_subscribed(true)).expect("subscribe audio");
        receiver.expect(|event| match event {
            NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == sid => {
                Some(())
            }
            _ => None,
        });
        let remote =
            remote_publication.remote_audio_track().expect("remote lookup").expect("audio track");
        let frames = block_on(remote.frames(1, 4)).expect("PCM sink");
        let capture = audio.clone();
        let producer = thread::spawn(move || {
            for n in 0..80 {
                let samples = (0..480)
                    .map(|i| {
                        (((n * 480 + i) as f64 * 440.0 * std::f64::consts::TAU / 48_000.0).sin()
                            * 8000.0) as i16
                    })
                    .collect();
                block_on(capture.capture_frame(NativeAudioFrame {
                    samples,
                    sample_rate: 48_000,
                    num_channels: 1,
                    samples_per_channel: 480,
                }))
                .expect("capture PCM");
            }
        });
        let mut audible = false;
        for _ in 0..80 {
            let frame = block_on(frames.next()).expect("decoded PCM").expect("audio frame");
            assert_eq!((frame.sample_rate, frame.num_channels), (48_000, 1));
            if frame.samples.iter().any(|s| s.unsigned_abs() > 1000) {
                audible = true;
                break;
            }
        }
        producer.join().expect("audio producer");
        assert!(audible, "synthetic audio did not survive Opus transport");
        let outgoing = block_on(audio.get_statistics()).expect("local audio stats");
        let first_outbound = outgoing
            .iter()
            .find_map(|s| match &s.stats {
                Some(crate::proto::rtc_stats::Stats::OutboundRtp(rtp))
                    if rtp.stream.kind == "audio" =>
                {
                    Some(rtp)
                }
                _ => None,
            })
            .expect("outbound audio RTP group");
        assert!(first_outbound.sent.bytes_sent > 0 && first_outbound.sent.packets_sent > 0);
        assert!(first_outbound.rtc.timestamp > 0 && !first_outbound.rtc.id.is_empty());
        assert!(outgoing.iter().any(|s| matches!(&s.stats, Some(crate::proto::rtc_stats::Stats::Codec(codec)) if codec.rtc.id == first_outbound.stream.codec_id)));
        let incoming = block_on(remote.get_statistics()).expect("remote audio stats");
        assert!(incoming.iter().any(|s| matches!(&s.stats, Some(crate::proto::rtc_stats::Stats::InboundRtp(rtp)) if rtp.inbound.bytes_received > 0 && rtp.received.packets_received > 0 && rtp.rtc.timestamp > 0)));
        assert!(!block_on(remote_publication.get_statistics()).unwrap().is_empty());

        if encrypted {
            receiver.expect(|event| match event {
                NativeRoomEvent::EncryptionStateChanged { state: EncryptionState::Ok, .. } => {
                    Some(())
                }
                _ => None,
            });
            assert!(sender
                .session
                .shared
                .room()
                .unwrap()
                .e2ee_manager()
                .frame_cryptors()
                .values()
                .all(|c| c.enabled()));
        }

        block_on(publication.set_muted(true)).expect("mute");
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackMuted { publication } if publication.sid() == sid => Some(()),
            _ => None,
        });
        block_on(publication.set_muted(false)).expect("unmute");
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackUnmuted { publication } if publication.sid() == sid => Some(()),
            _ => None,
        });
        let second_stats = block_on(audio.get_statistics()).unwrap();
        let second_outbound = second_stats
            .iter()
            .find_map(|s| match &s.stats {
                Some(crate::proto::rtc_stats::Stats::OutboundRtp(rtp))
                    if rtp.rtc.id == first_outbound.rtc.id =>
                {
                    Some(rtp)
                }
                _ => None,
            })
            .unwrap();
        assert!(second_outbound.rtc.timestamp >= first_outbound.rtc.timestamp);
        assert!(second_outbound.sent.bytes_sent >= first_outbound.sent.bytes_sent);
        frames.cancel();
        assert!(block_on(frames.next()).expect("cancelled sink").is_none());
        block_on(remote_publication.set_subscribed(false)).expect("unsubscribe");
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackUnsubscribed { publication } if publication.sid() == sid => {
                Some(())
            }
            _ => None,
        });
        // Core's unsubscribe event is optimistic, not an SFU acknowledgment. An immediate
        // false->true flip can coalesce away unsubscribe and never produce a new track event.
        block_on(publication.unpublish()).expect("unpublish audio");
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackUnpublished { publication } if publication.sid() == sid => {
                Some(())
            }
            _ => None,
        });
        let unsub = receiver.seen.iter().rposition(|e| matches!(e, NativeRoomEvent::TrackUnsubscribed { publication } if publication.sid() == sid)).expect("unsubscribed event");
        let unpub = receiver.seen.iter().rposition(|e| matches!(e, NativeRoomEvent::TrackUnpublished { publication } if publication.sid() == sid)).expect("unpublished event");
        assert!(unsub < unpub);
        block_on(audio.release()).expect("release source");

        let video = block_on(sender.session.create_video_track("pixels".into(), 320, 180, false))
            .expect("video source");
        let video_publication = block_on(video.publish(media_publish_options(TrackSource::Camera)))
            .expect("publish video");
        let video_info = video_publication.info().expect("video publication");
        assert_eq!(
            video_info.encryption_type(),
            if encrypted { EncryptionType::Gcm } else { EncryptionType::None }
        );
        let video_sid = video_info.sid();
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackPublished { publication } if publication.sid() == video_sid => {
                Some(())
            }
            _ => None,
        });
        let remote_video_publication = receiver
            .session
            .publication("publisher".into(), video_sid.clone())
            .expect("video lookup");
        block_on(remote_video_publication.set_subscribed(true)).expect("subscribe video");
        receiver.expect(|e| match e {
            NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == video_sid => {
                Some(())
            }
            _ => None,
        });
        let remote_video = receiver
            .session
            .publication("publisher".into(), video_sid)
            .expect("video lookup")
            .remote_video_track()
            .expect("remote video")
            .expect("subscribed");
        let video_frames = block_on(remote_video.frames(2)).expect("video sink");
        let capture = video.clone();
        let producer = thread::spawn(move || {
            for n in 0..40 {
                let data = [16u8, 32, 220, 255].repeat(320 * 180);
                block_on(capture.capture_frame(NativeVideoFrame {
                    format: NativeVideoPixelFormat::Bgra,
                    chroma_stride: None,
                    data,
                    width: 320,
                    height: 180,
                    stride: 1280,
                    timestamp_us: 1_000_000 + n * 50_000,
                    rotation: 0,
                }))
                .expect("capture video");
                thread::sleep(Duration::from_millis(50));
            }
        });
        let mut colored = false;
        for _ in 0..40 {
            let frame = block_on(video_frames.next()).expect("decode video").expect("video frame");
            if frame.data.chunks_exact(4).take(100).all(|p| p[2] > p[0].saturating_add(80)) {
                colored = true;
                break;
            }
        }
        producer.join().expect("video producer");
        assert!(colored, "synthetic color did not survive VP8 transport");
        assert!(block_on(video.get_statistics()).unwrap().iter().any(|s| matches!(&s.stats,
            Some(crate::proto::rtc_stats::Stats::OutboundRtp(rtp)) if rtp.stream.kind == "video" && rtp.outbound.frames_encoded > 0 && rtp.sent.bytes_sent > 0)));
        assert!(block_on(remote_video.get_statistics()).unwrap().iter().any(|s| matches!(&s.stats,
            Some(crate::proto::rtc_stats::Stats::InboundRtp(rtp)) if rtp.stream.kind == "video" && rtp.inbound.frames_decoded > 0)));

        let weak = Arc::downgrade(&receiver.session.shared);
        receiver.leave();
        assert!(weak.upgrade().is_none(), "media child retained disconnected session");
        assert!(block_on(video_frames.next()).is_err());
        assert!(remote_video.info().is_err());
        assert!(block_on(remote_video.get_statistics()).is_err());
        sender.leave();
        assert!(video.info().is_err());
    }
}

#[serial_test::serial]
#[test]
#[ignore = "requires physical microphone access; runs only when LK_TEST_MICROPHONE=1"]
fn real_device_microphone_to_remote_pcm() {
    assert_eq!(
        std::env::var("LK_TEST_MICROPHONE").as_deref(),
        Ok("1"),
        "physical capture must be explicitly enabled"
    );
    let name = unique("microphone");
    let sender =
        Peer::join_options(&name, "microphone", cursor_options(), encrypted_options(&name));
    let mut receiver =
        Peer::join_options(&name, "listener", cursor_options(), encrypted_options(&name));
    let audio = block_on(sender.session.create_microphone_track("device".into(), None))
        .expect("physical microphone");
    let publication = block_on(audio.publish(media_publish_options(TrackSource::Microphone)))
        .expect("publish physical microphone");
    let info = publication.info().expect("publication");
    assert_eq!(info.encryption_type(), EncryptionType::Gcm);
    let sid = info.sid();
    receiver.expect(|e| match e {
        NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == sid => Some(()),
        _ => None,
    });
    let remote = receiver
        .session
        .publication("microphone".into(), sid)
        .expect("publication")
        .remote_audio_track()
        .expect("remote")
        .expect("audio");
    let frames = block_on(remote.frames(1, 4)).expect("sink");
    let mut samples = 0;
    let mut peak = 0u16;
    for _ in 0..100 {
        let frame = block_on(frames.next()).expect("physical PCM").expect("frame");
        samples += frame.samples.len();
        peak = peak.max(frame.samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0));
    }
    assert!(samples >= 48_000, "less than one second captured");
    eprintln!("Encrypted physical microphone: {samples} decoded samples, peak {peak}; no PCM saved; speaker output not tested");
    block_on(publication.unpublish()).expect("unpublish microphone");
    block_on(audio.release()).expect("release microphone");
    sender.leave();
    receiver.leave();
}

#[test]
fn video_frame_layout_validation_and_nv12_conversion() {
    let frame = NativeVideoFrame {
        format: NativeVideoPixelFormat::Nv12,
        chroma_stride: Some(8),
        data: [vec![81; 8 * 4], [90, 240].repeat(8)].concat(),
        width: 4,
        height: 4,
        stride: 8,
        timestamp_us: 123,
        rotation: 90,
    };
    let output = NativeVideoFrame::from_core(frame.clone().into_core().unwrap()).unwrap();
    assert_eq!(
        (output.width, output.height, output.timestamp_us, output.rotation),
        (4, 4, 123, 90)
    );
    assert!(output.data.chunks_exact(4).all(|p| p[2] > 200 && p[0] < 30));
    let mut invalid = frame.clone();
    invalid.data.pop();
    assert!(invalid.into_core().is_err());
    let mut invalid = frame.clone();
    invalid.stride = u32::MAX;
    assert!(invalid.into_core().is_err());
    let mut invalid = frame;
    invalid.rotation = 45;
    assert!(invalid.into_core().is_err());
}

#[test]
fn encryption_provider_defaults_and_unsupported_options() {
    use super::encryption::{NativeKeyProvider, NativeKeyProviderOptions};
    let key = bytes::Bytes::from_static(b"test-only-shared-key");
    let provider =
        NativeKeyProvider::new(NativeKeyProviderOptions::default(), Some(key.clone())).unwrap();
    assert_eq!(provider.export_key(None, 0).unwrap(), Some(key));
    assert_eq!(provider.latest_key_index(), 0);
    let ratcheted = provider.ratchet_key(None, 0).unwrap().unwrap();
    assert_eq!(provider.export_key(None, 0).unwrap(), Some(ratcheted));
    assert!(provider.export_key(None, -1).is_err());
    assert!(NativeKeyProvider::new(NativeKeyProviderOptions::default(), None).is_err());
    assert!(NativeKeyProvider::new(
        NativeKeyProviderOptions { key_size: 256, ..Default::default() },
        None
    )
    .is_err());
    assert!(NativeKeyProvider::new(
        NativeKeyProviderOptions {
            discard_frame_when_cryptor_not_ready: true,
            ..Default::default()
        },
        None
    )
    .is_err());
    let individual = NativeKeyProvider::new(
        NativeKeyProviderOptions { shared_key: false, ..Default::default() },
        None,
    )
    .unwrap();
    individual
        .set_key(bytes::Bytes::from_static(b"participant-key"), Some("remote".into()), 0)
        .unwrap();
    assert_eq!(
        individual.export_key(Some("remote".into()), 0).unwrap().unwrap().as_ref(),
        b"participant-key"
    );
    assert!(individual.export_key(None, 0).is_err());
    let options =
        connect_options().with_encryption(Some(super::encryption::NativeEncryptionOptions {
            key_provider: NativeKeyProvider::new(
                NativeKeyProviderOptions::default(),
                Some(bytes::Bytes::from_static(b"test-only-key")),
            )
            .unwrap(),
            legacy_media_only: true,
        }));
    let core = options.0;
    assert!(core.encryption.is_none());
    #[allow(deprecated)]
    {
        assert!(core.e2ee.is_some());
    }
}

#[test]
fn typed_stats_preserve_large_numbers_groups_and_timestamps() {
    use livekit::webrtc::stats::{OutboundRtpStats, RtcStats};
    let mut core = OutboundRtpStats::default();
    core.rtc.id = "outbound".into();
    core.rtc.timestamp = 1_700_000_000_123_456;
    core.sent.bytes_sent = (1u64 << 53) + 1;
    core.stream.ssrc = 42;
    core.outbound.quality_limitation_durations.insert("cpu".into(), 0.125);
    let ffi: crate::proto::RtcStats = RtcStats::OutboundRtp(core).into();
    let Some(crate::proto::rtc_stats::Stats::OutboundRtp(outbound)) = ffi.stats else {
        panic!("group changed")
    };
    assert_eq!(outbound.rtc.timestamp, 1_700_000_000_123_456);
    assert_eq!(outbound.sent.bytes_sent, (1u64 << 53) + 1);
    assert_eq!(outbound.stream.ssrc, 42);
    assert_eq!(outbound.outbound.quality_limitation_durations["cpu"], 0.125);
}

#[test]
fn connect_core_options_preserve_nested_values_and_defaults() {
    let defaults = FfiRoomOptions::new();
    let options = defaults
        .with_rtc_config(
            vec![IceServer {
                urls: vec!["turn:127.0.0.1:3478".into()],
                username: "user".into(),
                password: "credential".into(),
            }],
            IceTransportPolicy::Relay,
            ContinualGatheringPolicy::GatherOnce,
            true,
        )
        .with_sdk("swift".into(), "test".into(), Some("components:1".into()))
        .with_connect_timeout(Duration::from_micros(1234567))
        .with_max_stream_payload_bytes(Some(1u64 << 33))
        .unwrap();
    let core = &options.0;
    assert_eq!(core.rtc_config.ice_servers[0].username, "user");
    assert_eq!(core.rtc_config.ice_servers[0].password, "credential");
    assert_eq!(core.rtc_config.ice_transport_type, IceTransportPolicy::Relay);
    assert_eq!(core.rtc_config.continual_gathering_policy, ContinualGatheringPolicy::GatherOnce);
    assert!(core.rtc_config.enable_sctp_snap);
    assert_eq!(core.sdk_options.other_sdks.as_deref(), Some("components:1"));
    assert_eq!(core.connect_timeout, Duration::from_micros(1234567));
    assert_eq!(defaults.connect_timeout(), RoomOptions::default().connect_timeout);
    assert!(defaults.ice_servers().is_empty(), "builder must not mutate prior options");
    let publishing = default_track_publish_options();
    assert_eq!(publishing.simulcast, TrackPublishOptions::default().simulcast);
    assert_eq!(publishing.frame_metadata_features, Default::default());
}

#[test]
fn structured_error_codes_are_not_derived_from_text() {
    use super::errors::*;
    use livekit_signaling::SignalError;
    let http = NativeSessionError::from(SignalError::Client(
        401.try_into().unwrap(),
        "not a classification string".into(),
    ));
    assert!(matches!(
        http,
        NativeSessionError::Signal {
            failure: NativeSignalFailure::Http {
                source: NativeHttpFailureSource::ClientValidation,
                status: 401,
                ..
            }
        }
    ));
    let handshake =
        NativeSessionError::from(SignalError::Handshake { status: 403.try_into().unwrap() });
    assert!(matches!(
        handshake,
        NativeSessionError::Signal {
            failure: NativeSignalFailure::Http {
                source: NativeHttpFailureSource::WebSocketHandshake,
                status: 403,
                ..
            }
        }
    ));
    assert!(matches!(
        NativeSessionError::from(RoomError::Request {
            reason: livekit_protocol::request_response::Reason::NotAllowed,
            message: "401 timeout unrelated text".into()
        }),
        NativeSessionError::RequestFailure { code: NativeRequestFailureCode::NotAllowed, .. }
    ));
    assert!(matches!(
        NativeSessionError::from(SignalError::Timeout("403".into())),
        NativeSessionError::Signal { failure: NativeSignalFailure::Timeout { .. } }
    ));
    assert!(matches!(
        NativeSessionError::from(SignalError::Connection("401 unauthorized".into())),
        NativeSessionError::Signal { failure: NativeSignalFailure::Transport { .. } }
    ));
}

#[serial_test::serial]
#[test]
#[ignore = "requires a local LiveKit server"]
fn real_structured_connection_and_stream_errors() {
    use super::errors::*;
    use crate::data_stream::common::{DataStreamError, StreamByteOptions, StreamIoErrorKind};
    let failed = FfiRoom::new();
    let error = block_on(failed.connect(
        env_or("LK_TEST_URL", "ws://127.0.0.1:7880"),
        "not-a-valid-token".into(),
        connect_options(),
    ))
    .expect_err("invalid token accepted");
    assert!(
        matches!(
            error,
            NativeSessionError::Signal {
                failure: NativeSignalFailure::Http {
                    status: 401,
                    source: NativeHttpFailureSource::ClientValidation,
                    ..
                }
            }
        ),
        "{error:?}"
    );
    block_on(failed.disconnect());
    let restricted = FfiRoom::new();
    let token = AccessToken::with_api_key(
        &env_or("LK_TEST_API_KEY", "devkey"),
        &env_or("LK_TEST_API_SECRET", "secret"),
    )
    .with_identity("restricted")
    .with_grants(VideoGrants {
        room_join: true,
        room: unique("permission"),
        can_update_own_metadata: Some(false),
        ..Default::default()
    })
    .to_jwt()
    .unwrap();
    block_on(restricted.connect(
        env_or("LK_TEST_URL", "ws://127.0.0.1:7880"),
        token,
        connect_options(),
    ))
    .unwrap();
    let error = block_on(restricted.set_name("not-allowed".into())).unwrap_err();
    assert!(
        matches!(
            error,
            NativeSessionError::RequestFailure { code: NativeRequestFailureCode::NotAllowed, .. }
        ),
        "{error:?}"
    );
    block_on(restricted.disconnect());
    let peer = Peer::join(&unique("typed-errors"), "sender");
    let error = block_on(
        peer.session
            .send_file(format!("/nonexistent/{}", unique("missing")), StreamByteOptions::default()),
    )
    .unwrap_err();
    assert!(
        matches!(error, DataStreamError::IoFailure { kind: StreamIoErrorKind::NotFound, .. }),
        "{error:?}"
    );
    peer.session.cancel();
    assert!(matches!(
        block_on(peer.session.send_bytes(bytes::Bytes::new(), StreamByteOptions::default())),
        Err(DataStreamError::Cancelled)
    ));
    block_on(peer.session.disconnect());
    let weak = Arc::downgrade(&peer.session.shared);
    drop(peer);
    assert!(weak.upgrade().is_none());
    let idle = FfiRoom::new();
    assert!(matches!(
        block_on(idle.send_bytes(bytes::Bytes::new(), StreamByteOptions::default())),
        Err(DataStreamError::InvalidState { .. })
    ));
    block_on(idle.disconnect());
}

#[test]
fn invalid_cursor_limits_fail_without_an_event() {
    for options in [limits(0, 100), limits(100, 0)] {
        let session = FfiRoom::new();
        let valid = session.events(cursor_options());
        let cursor = session.events(options);
        assert!(matches!(next(&cursor), Err(NativeSessionError::InvalidState { .. })));
        session.shared.apply(RoomEvent::TokenRefreshed { token: "still-delivered".into() });
        assert_eq!(next(&valid), Ok(Some(token_event("still-delivered"))));
    }
}

#[test]
fn unexplained_end_does_not_claim_client_initiated() {
    let session = FfiRoom::new();
    session.shared.update(Inner::finish);
    assert_eq!(session.state().disconnect_reason, Some(DisconnectReason::UnknownReason));
}

#[test]
fn secrets_are_not_debug_formatted() {
    let mut options = connect_options();
    Arc::make_mut(&mut options).0.rtc_config.ice_servers.push(IceServer {
        urls: vec!["turn:secret-url@example.test".into()],
        username: "secret-user".into(),
        password: "secret-password".into(),
    });
    let debug = format!("{options:?} {:?}", token_event("secret-token"));
    for secret in ["secret-url", "secret-user", "secret-password", "secret-token"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn decoded_video_accepts_odd_and_portrait_sizes() {
    use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
    for (width, height) in [(5, 3), (3, 2161)] {
        let core = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 42,
            frame_metadata: None,
            buffer: Box::new(I420Buffer::new(width, height)) as _,
        };
        let decoded = NativeVideoFrame::from_core(core).unwrap();
        assert_eq!((decoded.width, decoded.height), (width, height));
        assert_eq!(decoded.data.len(), (width * height * 4) as usize);
    }
}

#[inline(never)]
fn receive_then_pcm_privacy_checkpoint(phase: u32) {
    eprintln!("receive-then-PCM privacy checkpoint {phase}");
}

#[serial_test::serial]
#[test]
#[ignore = "requires a local server; native recording/acquisition breakpoints must remain unhit; never requests a microphone"]
fn real_receive_then_pcm_does_not_request_platform_audio() {
    let room = unique("receive-then-pcm");
    let sender = Peer::join(&room, "early");
    let early = block_on(sender.session.create_audio_track("early PCM".into(), 1)).unwrap();
    let publication =
        block_on(early.publish(media_publish_options(TrackSource::Microphone))).unwrap();
    let sid = publication.info().unwrap().sid();
    let mut receiver = Peer::join(&room, "receiver");
    receiver.expect(|event| match event {
        NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == sid => Some(()),
        _ => None,
    });
    receive_then_pcm_privacy_checkpoint(1);
    // The previously unsafe ordering: receive first, then publish manual PCM in
    // the receiving session and a different session. Do not create PlatformAudio
    // even to query it: its constructor itself enables capture globally.
    let local =
        block_on(receiver.session.create_audio_track("late PCM same room session".into(), 1))
            .unwrap();
    block_on(local.publish(media_publish_options(TrackSource::Microphone))).unwrap();
    let other = Peer::join(&room, "other");
    let late =
        block_on(other.session.create_audio_track("late PCM other session".into(), 1)).unwrap();
    block_on(late.publish(media_publish_options(TrackSource::Microphone))).unwrap();
    for _ in 0..30 {
        for track in [&early, &local, &late] {
            block_on(track.capture_frame(NativeAudioFrame {
                samples: vec![0; 480],
                sample_rate: 48000,
                num_channels: 1,
                samples_per_channel: 480,
            }))
            .unwrap();
        }
    }
    receive_then_pcm_privacy_checkpoint(2);
    other.leave();
    receiver.leave();
    sender.leave();
}

#[serial_test::serial]
#[test]
#[ignore = "requires a local server; synthetic video publication only, no devices"]
fn real_public_core_controls() {
    let name = unique("core-controls");
    let sender = Peer::join(&name, "controls-sender");
    let video =
        block_on(sender.session.create_video_track("control-video".into(), 640, 360, false))
            .unwrap();
    let mut options = media_publish_options(TrackSource::Camera);
    options.simulcast = true;
    let publication = block_on(video.publish(options)).unwrap();
    let sid = publication.info().unwrap().sid();
    let mut receiver = Peer::join(&name, "controls-receiver");
    receiver.expect(|event| match event {
        NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == sid => Some(()),
        _ => None,
    });
    let remote = receiver.session.publication("controls-sender".into(), sid.clone()).unwrap();
    assert!(remote.info().unwrap().simulcasted());
    block_on(remote.set_video_quality(VideoQuality::Low)).unwrap();
    block_on(remote.set_video_quality(VideoQuality::High)).unwrap();
    block_on(sender.session.publish_dtmf(1, "1".into(), vec!["controls-receiver".into()])).unwrap();
    receiver.expect(|event| match event {
        NativeRoomEvent::DtmfReceived {
            code: 1,
            digit: Some(digit),
            participant_identity: Some(identity),
        } if digit == "1" && identity == "controls-sender" => Some(()),
        _ => None,
    });
    receiver.leave();
    block_on(sender.session.set_track_subscription_permissions(false, vec![])).unwrap();
    let mut receiver = Peer::join(&name, "controls-receiver");
    let denied = receiver.session.publication("controls-sender".into(), sid.clone()).unwrap();
    // Observe a denied fresh subscription; core does not reliably clear the old
    // publication's attached-track/status projection when the server revokes it.
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        assert_ne!(
            denied.info().unwrap().remote().unwrap().subscription_status,
            SubscriptionStatus::Subscribed
        );
        thread::sleep(Duration::from_millis(20));
    }
    block_on(sender.session.set_track_subscription_permissions(
        false,
        vec![ParticipantTrackPermission {
            participant_identity: "controls-receiver".into(),
            allow_all: false,
            allowed_track_sids: vec![sid.clone().try_into().unwrap()],
        }],
    ))
    .unwrap();
    receiver.expect(|event| match event {
        NativeRoomEvent::TrackSubscribed { publication } if publication.sid() == sid => Some(()),
        _ => None,
    });
    let mut invalid_sid = Vec::new();
    <String as uniffi::FfiConverter<crate::UniFfiTag>>::write("invalid".into(), &mut invalid_sid);
    assert!(<TrackSid as uniffi::FfiConverter<crate::UniFfiTag>>::try_read(
        &mut invalid_sid.as_slice()
    )
    .is_err());
    receiver.leave();
    sender.leave();
    assert!(matches!(
        block_on(remote.set_video_quality(VideoQuality::Low)),
        Err(NativeSessionError::Closed)
    ));
}
