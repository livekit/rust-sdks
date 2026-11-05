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

use std::mem;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::Utc;
use livekit::prelude::ParticipantIdentity;
use livekit_session::{Message, MessageOrigin, Session, SessionEvent, SessionOptions};
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::a2a;
use crate::call::Call;
use crate::credentials::CallTokenSource;
use crate::directory::Directory;
use crate::error::{CallError, SendError};
use crate::feed::{lock, Shared, Update};
use crate::turn::Turn;

/// A change to a [`Thread`]'s feed or turns. The turn state is also
/// observed through [`Thread::state`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThreadEvent {
    /// A message was added, or updated in place (a growing answer or
    /// transcript).
    MessageUpserted(Message),
    /// A turn of the agent ended.
    TurnEnded(TurnOutcome),
}

/// How a turn of the agent ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The agent answered.
    Completed,
    /// The agent asked a question. The next [`Thread::send_text`] answers it.
    InputRequired,
    /// The turn was cancelled, e.g. because a newer message interrupted it.
    Canceled,
    /// The turn failed.
    Failed { reason: String },
}

/// What the agent is doing with the thread's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnState {
    /// No turn runs.
    #[default]
    Idle,
    /// The agent works on at least one message.
    Working,
}

impl TurnState {
    /// Whether no turn runs.
    pub fn is_idle(self) -> bool {
        self == Self::Idle
    }
}

/// The state of a [`Thread`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadState {
    /// What the agent is doing with the thread's text.
    pub turn: TurnState,
}

/// One conversation with an agent that lasts across text and calls, from a
/// [`ThreadDirectory`](crate::ThreadDirectory).
///
/// Text goes to the agent over A2A with [`send_text`](Self::send_text). A
/// [`Call`] joins a room with the agent for the same thread. The feed
/// ([`messages`](Self::messages), [`subscribe`](Self::subscribe)) merges the
/// history, the text turns and the call's transcripts.
///
/// [`end`](Self::end) or dropping the thread stops its turns and ends its
/// call. The thread itself stays on the server and can be opened again.
pub struct Thread {
    id: String,
    directory: Arc<Directory>,
    shared: Arc<Shared>,
    user: ParticipantIdentity,
    agent: ParticipantIdentity,
    owned: Mutex<Owned>,
}

/// The tasks and the call that the thread owns.
#[derive(Default)]
struct Owned {
    ended: bool,
    /// Aborted when the set drops with the thread.
    turns: JoinSet<()>,
    call: Option<CallSlot>,
}

struct CallSlot {
    call: Call,
    /// Copies the call's transcripts into the feed.
    forward: JoinHandle<()>,
}

impl Thread {
    pub(crate) fn new(directory: Arc<Directory>, id: String, history: Vec<Update>) -> Self {
        Self {
            user: ParticipantIdentity(directory.options.scope.clone()),
            agent: ParticipantIdentity(directory.options.agent_name.clone()),
            shared: Arc::new(Shared::new(history)),
            owned: Mutex::default(),
            directory,
            id,
        }
    }

    /// The thread id (`AT_...`).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The message feed, ordered by [`Message::seq`].
    pub fn messages(&self) -> Vec<Message> {
        self.shared.messages()
    }

    /// A receiver of changes to the feed and of ended turns.
    ///
    /// Unbounded, like [`Session::subscribe`]. Drop the receiver to
    /// unsubscribe. The receiver ends when the thread ends.
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<ThreadEvent> {
        self.shared.subscribe()
    }

    /// A receiver of the thread's state. [`borrow`](watch::Receiver::borrow)
    /// reads the current value, and [`wait_for`](watch::Receiver::wait_for)
    /// awaits a condition.
    pub fn state(&self) -> watch::Receiver<ThreadState> {
        self.shared.watch()
    }

    /// Sends `text` to the agent, also while a call runs.
    ///
    /// Returns when the agent accepted the message, which is then in the feed
    /// as [`MessageOrigin::UserInput`]. The answer arrives in the feed as
    /// [`MessageOrigin::AgentText`], and the turn ends with
    /// [`ThreadEvent::TurnEnded`]. A new message interrupts a turn that still
    /// runs. After [`TurnOutcome::InputRequired`], the message answers the
    /// agent's question.
    pub async fn send_text(&self, text: &str) -> Result<Message, SendError> {
        if lock(&self.owned).ended {
            return Err(SendError::Ended);
        }

        let message_id = new_id("msg");
        let input_task = self.shared.input_task();
        let body =
            a2a::stream_request(&new_id("rpc"), &message_id, &self.id, input_task.as_deref(), text);
        let token = self.directory.api_key.thread_token(&self.directory.options.scope, &self.id)?;
        let mut frames = a2a::open(&self.directory.endpoint_url, &token, body).await?;

        // The first frame says whether the agent accepted the message.
        let first = frames.next().await?.ok_or(SendError::EmptyResponse)?;
        if let Some(error) = first.error {
            return Err(SendError::Rejected { code: error.code, reason: error.reason });
        }

        let mut owned = lock(&self.owned);
        if owned.ended {
            return Err(SendError::Ended);
        }
        let message = self.shared.upsert(Update {
            id: message_id,
            timestamp: Utc::now(),
            origin: MessageOrigin::UserInput,
            participant_identity: self.user.clone(),
            text: text.to_owned(),
            is_final: true,
        });
        self.shared.begin_turn();

        let mut turn = Turn::new(Arc::clone(&self.shared), self.agent.clone());
        if let Some(outcome) = turn.apply(first) {
            turn.end(outcome);
        } else {
            // Turns that ended are removed, so the set does not grow.
            while owned.turns.try_join_next().is_some() {}
            owned.turns.spawn(turn.run(frames));
        }
        Ok(message)
    }

    /// Creates the thread's call, which joins a room with the agent on
    /// [`Call::start`]. The thread has one call; a second one is
    /// [`CallError::AlreadyActive`].
    ///
    /// Must be called within a Tokio runtime.
    pub fn create_call(&self, options: SessionOptions) -> Result<Call, CallError> {
        let mut owned = lock(&self.owned);
        if owned.ended {
            return Err(CallError::ThreadEnded);
        }
        if owned.call.is_some() {
            return Err(CallError::AlreadyActive);
        }

        let source = CallTokenSource::new(Arc::clone(&self.directory), &self.id);
        let session = Session::new(source, options);
        let forward =
            tokio::spawn(forward_transcripts(session.subscribe(), Arc::clone(&self.shared)));
        let call = Call::new(session);
        owned.call = Some(CallSlot { call: call.clone(), forward });
        Ok(call)
    }

    /// The thread's call, if [`create_call`](Self::create_call) made one.
    pub fn call(&self) -> Option<Call> {
        lock(&self.owned).call.as_ref().map(|slot| slot.call.clone())
    }

    /// Ends the thread: stops its turns, ends its call and ends the feed.
    /// Later sends fail with [`SendError::Ended`].
    pub async fn end(&self) {
        let (mut turns, call) = {
            let mut owned = lock(&self.owned);
            if owned.ended {
                return;
            }
            owned.ended = true;
            (mem::take(&mut owned.turns), owned.call.take())
        };
        turns.shutdown().await;
        if let Some(slot) = call {
            slot.call.close();
            slot.call.end().await;
            slot.forward.abort();
            // An aborted task ends with a cancelled error; nothing to report.
            let _ = slot.forward.await;
        }
        self.shared.close();
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        let owned = self.owned.get_mut().unwrap_or_else(PoisonError::into_inner);
        if owned.ended {
            return;
        }
        owned.ended = true;
        // The turns stop when their `JoinSet` drops with the thread.
        if let Some(slot) = owned.call.take() {
            slot.forward.abort();
            slot.call.close();
            // The app may still hold the call, which keeps its session alive,
            // so end it. Ending is async and finishes on its own; without a
            // runtime, the session ends when the app drops its call.
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move { slot.call.end().await });
            }
        }
        self.shared.close();
    }
}

/// Copies the call's messages into the thread's feed until the thread aborts
/// this task.
async fn forward_transcripts(
    mut events: mpsc::UnboundedReceiver<SessionEvent>,
    shared: Arc<Shared>,
) {
    while let Some(event) = events.recv().await {
        if let SessionEvent::MessageUpserted(message) = event {
            shared.upsert(Update::from(message));
        }
    }
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{:032x}", rand::random::<u128>())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{ThreadDirectory, ThreadDirectoryOptions};

    fn thread() -> Thread {
        let directory = ThreadDirectory::with_api_key(
            "wss://example.invalid",
            "key",
            "secret",
            ThreadDirectoryOptions {
                agent_name: "agent".into(),
                endpoint: "chat".into(),
                scope: "user".into(),
                deployment: None,
            },
        );
        Thread::new(Arc::clone(&directory.inner), "AT_test".into(), Vec::new())
    }

    /// Lets aborted tasks drop their futures.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn dropping_the_thread_frees_its_feed_and_closes_its_call() {
        let thread = thread();
        let call = thread.create_call(SessionOptions::default()).unwrap();
        assert!(matches!(
            thread.create_call(SessionOptions::default()),
            Err(CallError::AlreadyActive)
        ));
        let shared = Arc::downgrade(&thread.shared);
        let mut events = thread.subscribe();

        drop(thread);
        settle().await;

        assert!(events.recv().await.is_none());
        // The forwarding task held the last other reference to the feed.
        assert!(shared.upgrade().is_none());
        assert!(matches!(call.start().await, Err(CallError::ThreadEnded)));
    }

    #[tokio::test]
    async fn ending_the_thread_stops_sends_and_calls() {
        let thread = thread();
        let call = thread.create_call(SessionOptions::default()).unwrap();
        let mut events = thread.subscribe();

        thread.end().await;

        assert!(events.recv().await.is_none());
        assert!(thread.call().is_none());
        assert!(matches!(thread.send_text("Hi").await, Err(SendError::Ended)));
        assert!(matches!(
            thread.create_call(SessionOptions::default()),
            Err(CallError::ThreadEnded)
        ));
        assert!(matches!(call.start().await, Err(CallError::ThreadEnded)));
        assert_eq!(Arc::strong_count(&thread.shared), 1);
    }
}
