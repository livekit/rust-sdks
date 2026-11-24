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

//! The bounded event backlog between the room driver and the foreign consumer.

use std::{collections::VecDeque, future::Future, sync::Arc};

use parking_lot::Mutex;
use tokio::sync::Notify;

use super::records::{
    NativeDataPacketKind, NativeEventCursorOptions, NativeRoomEvent, NativeRoomState,
    NativeRoomUpdate, NativeSessionError,
};

/// Waits until `check` returns a value. `notify` must fire after each change
/// that `check` can observe.
///
/// The wait registers before it checks, so a change between the check and the
/// sleep is not lost. It needs no runtime, so a foreign executor polls it in
/// place.
pub(super) async fn wait_for<T>(notify: &Notify, mut check: impl FnMut() -> Option<T>) -> T {
    loop {
        let notified = notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if let Some(value) = check() {
            return value;
        }
        notified.await;
    }
}

/// Identifies events where only the newest value matters.
#[derive(PartialEq)]
enum CoalesceKey<'a> {
    Room,
    Speakers,
    Quality(&'a str),
    Participant(&'a str),
}

impl NativeRoomEvent {
    fn coalesce_key(&self) -> Option<CoalesceKey<'_>> {
        match self {
            Self::RoomUpdated { .. } => Some(CoalesceKey::Room),
            Self::ActiveSpeakersChanged { .. } => Some(CoalesceKey::Speakers),
            Self::ConnectionQualityChanged { participant } => {
                Some(CoalesceKey::Quality(&participant.identity))
            }
            Self::ParticipantUpdated { participant } => {
                Some(CoalesceKey::Participant(&participant.identity))
            }
            _ => None,
        }
    }

    /// Payload bytes that count toward the byte limit.
    fn cost(&self) -> usize {
        match self {
            Self::DataReceived { packet } => packet.payload.len(),
            Self::RpcRequested { invocation } => invocation.payload_bytes(),
            _ => 0,
        }
    }

    fn is_lossy(&self) -> bool {
        matches!(self, Self::DataReceived { packet } if packet.kind == NativeDataPacketKind::Lossy)
    }
}

enum End {
    /// The session ended. Items that remain are still delivered.
    Finished,
    Failed(NativeSessionError),
}

#[derive(Default)]
struct Backlog {
    items: VecDeque<NativeRoomUpdate>,
    bytes: usize,
    end: Option<End>,
    dropped_lossy: u64,
}

/// A bounded, ordered backlog of room events.
pub(super) struct EventQueue {
    backlog: Mutex<Backlog>,
    changed: Notify,
    limits: NativeEventCursorOptions,
}

impl EventQueue {
    pub(super) fn new(limits: NativeEventCursorOptions) -> Self {
        let queue = Self { backlog: Mutex::default(), changed: Notify::new(), limits };
        if limits.max_items == 0 || limits.max_bytes == 0 {
            queue.fail(NativeSessionError::InvalidState {
                reason: "event cursor limits must both be greater than zero".into(),
            });
        }
        queue
    }

    /// Adds an event. Never waits for the consumer.
    ///
    /// When the backlog is full, a lossy data packet is dropped and counted.
    /// Any other event fails the queue with
    /// [`NativeSessionError::Overflow`], because a silent drop would corrupt
    /// the consumer's view of the room.
    pub(super) fn push(&self, update: NativeRoomUpdate) {
        let mut backlog = self.backlog.lock();
        if backlog.end.is_some() {
            return;
        }
        // Search only the trailing run of coalescable events, so the newer
        // value never moves across an ordered event.
        // ponytail: linear scan, bounded by max_items; index by key if it shows in a profile.
        let from_back = update.event.coalesce_key().and_then(|key| {
            backlog
                .items
                .iter()
                .rev()
                .map(|item| item.event.coalesce_key())
                .take_while(Option::is_some)
                .position(|queued| queued.as_ref() == Some(&key))
        });
        if let Some(from_back) = from_back {
            // The newer value goes to the back, not into the old position, so
            // the revisions of the backlog never go back.
            let index = backlog.items.len() - 1 - from_back;
            backlog.items.remove(index);
            backlog.items.push_back(update);
            return;
        }
        let cost = update.event.cost();
        let full = backlog.items.len() >= self.limits.max_items as usize
            || (backlog.bytes + cost) as u64 > self.limits.max_bytes;
        if full && update.event.is_lossy() {
            backlog.dropped_lossy += 1;
            return;
        }
        if full {
            let error = NativeSessionError::Overflow {
                items: backlog.items.len() as u32,
                bytes: backlog.bytes as u64,
            };
            log::error!("room event cursor failed: {error}");
            backlog.items = VecDeque::new();
            backlog.bytes = 0;
            backlog.end = Some(End::Failed(error));
        } else {
            backlog.bytes += cost;
            backlog.items.push_back(update);
        }
        drop(backlog);
        self.changed.notify_waiters();
    }

    /// Marks the end of the session. Items that remain are still delivered.
    pub(super) fn finish(&self) {
        self.end(End::Finished);
    }

    /// Releases the backlog and fails every current and later `next` call.
    pub(super) fn fail(&self, error: NativeSessionError) {
        self.end(End::Failed(error));
    }

    fn end(&self, end: End) {
        let mut backlog = self.backlog.lock();
        if backlog.end.is_some() {
            return;
        }
        if matches!(end, End::Failed(_)) {
            backlog.items = VecDeque::new();
            backlog.bytes = 0;
        }
        backlog.end = Some(end);
        drop(backlog);
        self.changed.notify_waiters();
    }

    fn next(
        &self,
    ) -> impl Future<Output = Result<Option<NativeRoomUpdate>, NativeSessionError>> + '_ {
        wait_for(&self.changed, || {
            let mut backlog = self.backlog.lock();
            if let Some(End::Failed(error)) = &backlog.end {
                return Some(Err(error.clone()));
            }
            if let Some(update) = backlog.items.pop_front() {
                backlog.bytes -= update.event.cost();
                return Some(Ok(Some(update)));
            }
            backlog.end.is_some().then_some(Ok(None))
        })
    }
}

/// An ordered cursor over the events of one room session.
///
/// Get it from [`NativeRoomSession::events`](super::NativeRoomSession::events).
/// To release the cursor without a call to [`cancel`](Self::cancel) has the
/// same effect as the call.
#[derive(uniffi::Object)]
pub struct NativeRoomEventCursor {
    queue: Arc<EventQueue>,
    start: NativeRoomState,
}

impl NativeRoomEventCursor {
    pub(super) fn new(queue: Arc<EventQueue>, start: NativeRoomState) -> Arc<Self> {
        Arc::new(Self { queue, start })
    }

    /// Returns `false` after the backlog finished or failed. Does not read.
    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        self.queue.backlog.lock().end.is_none()
    }
}

#[uniffi::export]
impl NativeRoomEventCursor {
    /// Returns the next event, or `None` after the last event of the session.
    ///
    /// Fails with [`NativeSessionError::Cancelled`] after a cancel, and with
    /// [`NativeSessionError::Overflow`] when the consumer was too slow. Both
    /// are final for this cursor. The session stays usable after an overflow:
    /// get a new cursor and start again from its
    /// [`start_state`](Self::start_state).
    pub async fn next(&self) -> Result<Option<NativeRoomUpdate>, NativeSessionError> {
        self.queue.next().await
    }

    /// Returns the snapshot from the moment the cursor was created.
    ///
    /// For a connected session, subsequent events follow this snapshot. A cursor
    /// created before connect starts at Idle: consume the snapshot returned by
    /// `connect` before reading events; initial entities are not replayed here.
    pub fn start_state(&self) -> NativeRoomState {
        self.start.clone()
    }

    /// Ends the cursor, releases its backlog and wakes a pending
    /// [`next`](Self::next). Safe to call more than once.
    ///
    /// With UniFFI 0.31, do not call this from a Swift `onCancel` handler.
    /// The call wakes a pending future, and that can deadlock in the handler.
    pub fn cancel(&self) {
        self.queue.fail(NativeSessionError::Cancelled);
    }

    /// Number of lossy data packets that were dropped because the backlog was full.
    pub fn dropped_lossy_packets(&self) -> u64 {
        self.queue.backlog.lock().dropped_lossy
    }
}

impl Drop for NativeRoomEventCursor {
    fn drop(&mut self) {
        self.cancel();
    }
}
