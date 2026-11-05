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

//! The state a thread shares with its tasks: the message feed, its
//! subscribers and the turn state.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Utc};
use livekit::prelude::ParticipantIdentity;
use livekit_session::{Message, MessageOrigin};
use tokio::sync::{mpsc, watch};

use crate::thread::{ThreadEvent, ThreadState, TurnOutcome, TurnState};

/// A message as a source reports it, before the feed assigns its `seq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Update {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub origin: MessageOrigin,
    pub participant_identity: ParticipantIdentity,
    pub text: String,
    pub is_final: bool,
}

impl From<Message> for Update {
    fn from(message: Message) -> Self {
        Self {
            id: message.id,
            timestamp: message.timestamp,
            origin: message.origin,
            participant_identity: message.participant_identity,
            text: message.text,
            is_final: message.is_final,
        }
    }
}

/// The messages of a thread in receive order. A message is identified by its
/// id and participant, like in a session, and is updated in place.
#[derive(Default)]
struct Store {
    messages: Vec<Message>,
    index: HashMap<(String, ParticipantIdentity), usize>,
    next_seq: u64,
}

impl Store {
    fn upsert(&mut self, update: Update) -> Message {
        let key = (update.id, update.participant_identity);
        if let Some(&position) = self.index.get(&key) {
            let message = &mut self.messages[position];
            message.timestamp = update.timestamp;
            message.origin = update.origin;
            message.text = update.text;
            message.is_final = update.is_final;
            return message.clone();
        }

        let (id, participant_identity) = key.clone();
        let message = Message {
            id,
            seq: self.next_seq,
            timestamp: update.timestamp,
            origin: update.origin,
            participant_identity,
            text: update.text,
            is_final: update.is_final,
        };
        self.next_seq += 1;
        self.index.insert(key, self.messages.len());
        self.messages.push(message.clone());
        message
    }
}

#[derive(Default)]
struct Feed {
    store: Store,
    /// Unbounded, like a session's: the thread must never block on a slow
    /// subscriber, and the rate is bounded by the conversation.
    subscribers: Vec<mpsc::UnboundedSender<ThreadEvent>>,
    closed: bool,
}

impl Feed {
    /// Sends `event` to every subscriber, and removes those that dropped
    /// their receiver.
    fn dispatch(&mut self, event: &ThreadEvent) {
        self.subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
}

#[derive(Default)]
struct Turns {
    /// Turns that the agent accepted and that have not ended.
    active: usize,
    /// The task that waits for the user's input, which the next message
    /// continues.
    input_task: Option<String>,
}

pub(crate) struct Shared {
    feed: Mutex<Feed>,
    turns: Mutex<Turns>,
    state: watch::Sender<ThreadState>,
}

impl Shared {
    pub(crate) fn new(history: Vec<Update>) -> Self {
        let mut store = Store::default();
        for update in history {
            store.upsert(update);
        }
        Self {
            feed: Mutex::new(Feed { store, ..Default::default() }),
            turns: Mutex::default(),
            state: watch::Sender::new(ThreadState::default()),
        }
    }

    /// Adds or updates a message and notifies subscribers. The event is sent
    /// under the feed's lock, so subscribers see changes in feed order.
    pub(crate) fn upsert(&self, update: Update) -> Message {
        let mut feed = lock(&self.feed);
        let message = feed.store.upsert(update);
        feed.dispatch(&ThreadEvent::MessageUpserted(message.clone()));
        message
    }

    pub(crate) fn messages(&self) -> Vec<Message> {
        lock(&self.feed).store.messages.clone()
    }

    pub(crate) fn subscribe(&self) -> mpsc::UnboundedReceiver<ThreadEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut feed = lock(&self.feed);
        // After close, the sender drops here and the receiver ends at once.
        if !feed.closed {
            feed.subscribers.push(sender);
        }
        receiver
    }

    pub(crate) fn watch(&self) -> watch::Receiver<ThreadState> {
        self.state.subscribe()
    }

    pub(crate) fn input_task(&self) -> Option<String> {
        lock(&self.turns).input_task.clone()
    }

    /// Starts a turn that the agent accepted. A message continues the task
    /// that waited for input, so that task no longer waits.
    pub(crate) fn begin_turn(&self) {
        let mut turns = lock(&self.turns);
        turns.input_task = None;
        turns.active += 1;
        self.set_turn(TurnState::Working);
    }

    /// Ends a turn: publishes its outcome, and the idle state when no other
    /// turn runs.
    pub(crate) fn end_turn(&self, outcome: TurnOutcome, task_id: Option<String>) {
        {
            let mut turns = lock(&self.turns);
            turns.active = turns.active.saturating_sub(1);
            if outcome == TurnOutcome::InputRequired {
                turns.input_task = task_id;
            }
            if turns.active == 0 {
                self.set_turn(TurnState::Idle);
            }
        }
        lock(&self.feed).dispatch(&ThreadEvent::TurnEnded(outcome));
    }

    /// Ends the feed: subscribers' receivers end, and the state is idle.
    pub(crate) fn close(&self) {
        {
            let mut feed = lock(&self.feed);
            feed.closed = true;
            feed.subscribers.clear();
        }
        *lock(&self.turns) = Turns::default();
        self.set_turn(TurnState::Idle);
    }

    fn set_turn(&self, turn: TurnState) {
        self.state.send_if_modified(|state| {
            let changed = state.turn != turn;
            state.turn = turn;
            changed
        });
    }
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(id: &str, text: &str, is_final: bool) -> Update {
        Update {
            id: id.into(),
            timestamp: DateTime::default(),
            origin: MessageOrigin::AgentText,
            participant_identity: ParticipantIdentity("agent".into()),
            text: text.into(),
            is_final,
        }
    }

    #[test]
    fn upsert_updates_in_place_after_history() {
        let shared = Shared::new(vec![update("old", "earlier", true)]);
        let mut events = shared.subscribe();

        assert_eq!(shared.upsert(update("a", "Hel", false)).seq, 1);
        assert_eq!(shared.upsert(update("a", "Hello", true)).seq, 1);

        let texts: Vec<_> = shared.messages().into_iter().map(|m| m.text).collect();
        assert_eq!(texts, ["earlier", "Hello"]);
        assert!(
            matches!(events.try_recv(), Ok(ThreadEvent::MessageUpserted(m)) if m.text == "Hel")
        );
        assert!(
            matches!(events.try_recv(), Ok(ThreadEvent::MessageUpserted(m)) if m.text == "Hello")
        );
    }

    #[test]
    fn turn_state_is_working_until_the_last_turn_ends() {
        let shared = Shared::new(Vec::new());
        let state = shared.watch();
        shared.begin_turn();
        shared.begin_turn();
        shared.end_turn(TurnOutcome::Canceled, None);
        assert_eq!(state.borrow().turn, TurnState::Working);
        shared.end_turn(TurnOutcome::InputRequired, Some("t".into()));
        assert!(state.borrow().turn.is_idle());
        assert_eq!(shared.input_task().as_deref(), Some("t"));

        shared.begin_turn();
        assert_eq!(shared.input_task(), None);
    }

    #[test]
    fn close_ends_every_receiver() {
        let shared = Shared::new(Vec::new());
        let mut before = shared.subscribe();
        shared.close();
        let mut after = shared.subscribe();
        assert!(before.try_recv().is_err() && before.is_closed());
        assert!(after.try_recv().is_err() && after.is_closed());
    }
}
