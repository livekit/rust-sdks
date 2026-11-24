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

fn block_on<T>(future: impl Future<Output = T>) -> T {
    poll_for(LIMIT, future).expect("the wait did not end in time")
}

fn limits(max_items: u32, max_bytes: u64) -> NativeEventCursorOptions {
    NativeEventCursorOptions { max_items, max_bytes }
}

fn cursor_options() -> NativeEventCursorOptions {
    limits(4096, 16 * 1024 * 1024)
}

fn connect_options() -> NativeConnectOptions {
    NativeConnectOptions {
        auto_subscribe: true,
        adaptive_stream: false,
        dynacast: false,
        single_peer_connection: true,
        join_retries: 0,
        connect_timeout_ms: Some(2_000),
        sdk: None,
        max_stream_payload_bytes: None,
        sdk_version: None,
    }
}

fn update(revision: u64, event: NativeRoomEvent) -> NativeRoomUpdate {
    NativeRoomUpdate { revision, event }
}

fn token_event(token: &str) -> NativeRoomEvent {
    NativeRoomEvent::TokenRefreshed { token: token.to_owned() }
}

fn room_event(metadata: &str) -> NativeRoomEvent {
    NativeRoomEvent::RoomUpdated {
        room: NativeRoomInfo {
            sid: None,
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

fn data_event(kind: NativeDataPacketKind, size: usize) -> NativeRoomEvent {
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
    NativeRoomEventCursor::new(queue.clone(), NativeRoomSession::new().state())
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
    queue.push(update(1, data_event(NativeDataPacketKind::Reliable, 8)));

    queue.push(update(1, data_event(NativeDataPacketKind::Lossy, 8)));
    assert_eq!(cursor.dropped_lossy_packets(), 1);
    assert_eq!(next(&cursor), Ok(Some(data_event(NativeDataPacketKind::Reliable, 8))));

    // The read released the bytes.
    queue.push(update(1, data_event(NativeDataPacketKind::Reliable, 8)));
    queue.push(update(1, data_event(NativeDataPacketKind::Reliable, 8)));
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
    let session = NativeRoomSession::new();
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
    let session = NativeRoomSession::new();
    let cursor = session.events(cursor_options());
    block_on(session.disconnect());
    block_on(session.disconnect());

    assert_eq!(
        next(&cursor),
        Ok(Some(NativeRoomEvent::Disconnected { reason: NativeDisconnectReason::ClientInitiated }))
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
    let session = NativeRoomSession::new();
    let cursor = session.events(cursor_options());

    let result =
        block_on(session.connect(format!("ws://127.0.0.1:{port}"), "t".into(), connect_options()));
    assert!(matches!(result, Err(NativeSessionError::Connection { .. })), "{result:?}");

    assert_eq!(
        next(&cursor),
        Ok(Some(NativeRoomEvent::Disconnected { reason: NativeDisconnectReason::JoinFailure }))
    );
    assert_eq!(next(&cursor), Ok(None));
    block_on(session.disconnect());
    let state = session.state();
    assert_eq!(state.connection, NativeConnectionState::Disconnected);
    assert_eq!(state.disconnect_reason, Some(NativeDisconnectReason::JoinFailure));

    let shared = Arc::downgrade(&session.shared);
    drop((session, cursor));
    assert!(shared.upgrade().is_none(), "the session leaked");
}

#[test]
fn cancel_during_connect_returns_at_once() {
    // The listener accepts the TCP connection in its backlog and never
    // answers, so the signal handshake waits for its timeout.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let session = NativeRoomSession::new();
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
        let session = NativeRoomSession::new();
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
    session: Arc<NativeRoomSession>,
    cursor: Arc<NativeRoomEventCursor>,
    /// The snapshot that `connect` returned. It never changes afterwards.
    first: NativeRoomState,
    /// The revision of the last event that this peer read.
    revision: u64,
    seen: Vec<NativeRoomEvent>,
}

impl Peer {
    fn join(room: &str, identity: &str) -> Self {
        Self::join_with(room, identity, cursor_options())
    }

    fn join_with(room: &str, identity: &str, cursor: NativeEventCursorOptions) -> Self {
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

        let session = NativeRoomSession::new();
        let cursor = session.events(cursor);
        let url = env_or("LK_TEST_URL", "ws://127.0.0.1:7880");
        let options =
            NativeConnectOptions { connect_timeout_ms: Some(10_000), ..connect_options() };
        let first = block_on(session.connect(url, token, options)).expect("connect");

        assert_eq!(first.connection, NativeConnectionState::Connected);
        assert_eq!(first.room.as_ref().map(|room| room.name.as_str()), Some(room));
        let local = first.local_participant.as_ref().expect("local participant");
        assert_eq!(local.identity, identity);
        assert!(local.is_local);
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
        let reason = NativeDisconnectReason::ClientInitiated;
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

fn remote_identities(state: &NativeRoomState) -> Vec<&str> {
    let mut identities: Vec<_> =
        state.remote_participants.iter().map(|p| p.identity.as_str()).collect();
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
    assert_eq!((seen_bob.identity.as_str(), seen_bob.is_local), ("bob", false));
    assert!(alice.revision > alice.first.revision);
    assert_eq!(carol.expect(joined).identity, "dave");

    let mut generations: Vec<u64> =
        [&alice, &bob, &carol, &dave].iter().map(|peer| peer.first.generation).collect();
    generations.dedup();
    assert_eq!(generations.len(), 4, "each session has its own generation");

    // Reliable data, in order, with topic and sender.
    for index in 0..50u8 {
        alice.send(&[b'a', index], true, Some("topic-a"));
    }
    for index in 0..50u8 {
        let packet = bob.expect_data();
        assert_eq!(packet.payload, [b'a', index]);
        assert_eq!(packet.kind, NativeDataPacketKind::Reliable);
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
    assert_eq!(lossy.kind, NativeDataPacketKind::Lossy);
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
    assert_eq!((changed, participant.identity.as_str()), (attributes.clone(), "alice"));
    assert_eq!(participant.attributes, attributes);
    bob.expect(|event| match event {
        NativeRoomEvent::ParticipantNameChanged { participant, .. } => {
            (participant.name == "Alice A").then_some(())
        }
        _ => None,
    });
    assert!(bob.first.remote_participants[0].attributes.is_empty());
    let newer = block_on(bob.session.state_changed(bob.first.revision)).expect("newer state");
    assert!(newer.revision > bob.first.revision);
    assert_eq!(newer.generation, bob.first.generation);
    let current = bob.session.state();
    assert_eq!(current.remote_participants[0].attributes, attributes);
    assert_eq!(current.remote_participants[0].name, "Alice A");

    // Room A ends for alice. Bob sees it, and room B keeps working.
    let alice_events = alice.leave();
    let left = bob.expect(|event| match event {
        NativeRoomEvent::ParticipantDisconnected { participant } => Some(participant.clone()),
        _ => None,
    });
    assert_eq!(left.identity, "alice");
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
        let victim = NativeRoomSession::new();
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
    let session = NativeRoomSession::new();
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
    assert_eq!(session.state(), frozen);
    session.shared.update(Inner::finish);
    block_on(session.disconnect());
}

#[test]
fn session_joins_commands_when_callers_drop_and_on_cancel() {
    for drop_caller in [false, true] {
        let session = NativeRoomSession::new();
        let resource = Arc::new(());
        let weak = Arc::downgrade(&resource);
        let (started, ready) = std::sync::mpsc::channel();
        let mut call = Box::pin(session.shared.command(async move {
            let _resource = resource;
            started.send(()).expect("started");
            std::future::pending::<()>().await;
        }));
        assert!(poll_for(Duration::from_millis(20), &mut call).is_none());
        ready.recv_timeout(LIMIT).expect("command started");
        if drop_caller {
            drop(call);
        } else {
            session.cancel();
            assert_eq!(block_on(call), Err(NativeSessionError::Cancelled));
        }
        session.cancel();
        block_on(session.disconnect());
        assert!(weak.upgrade().is_none());
        assert!(session.shared.commands.lock().is_empty());
        assert_eq!(block_on(session.shared.command(async {})), Err(NativeSessionError::Cancelled));
    }
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
    for _ in 0..2 {
        let room = unique("helpers");
        let sender = Peer::join(&room, "sender");
        let mut receiver = Peer::join(&room, "receiver");
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
        assert!(receiver.session.shared.commands.lock().is_empty());

        let held_room = sender.session.shared.room().expect("connected room");
        let weak_room = Arc::downgrade(&held_room);
        let mut command = Box::pin(sender.session.shared.command(async move {
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
