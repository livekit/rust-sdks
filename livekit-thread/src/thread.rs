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
use livekit_session::{Message, MessageOrigin, Session, SessionEvent, SessionState};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::a2a;
use crate::call::Call;
use crate::credentials::CallTokenSource;
use crate::directory::{Directory, ThreadInfo};
use crate::error::SendError;
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
    /// The connection and agent state of the thread's [`Call`].
    pub call: SessionState,
}

/// One conversation with an agent that lasts across text and calls, from a
/// [`ThreadDirectory`](crate::ThreadDirectory).
///
/// Text goes to the agent over A2A with [`send_text`](Self::send_text). The
/// thread's [`call`](Self::call) joins a room with the agent for the same
/// thread. The feed ([`messages`](Self::messages),
/// [`subscribe`](Self::subscribe)) merges the history, the text turns and the
/// call's transcripts, and [`state`](Self::state) covers the turns and the
/// call.
///
/// [`end`](Self::end) or dropping the thread stops its turns and ends its
/// call. The thread itself stays on the server and can be opened again.
pub struct Thread {
    info: ThreadInfo,
    directory: Arc<Directory>,
    shared: Arc<Shared>,
    user: ParticipantIdentity,
    agent: ParticipantIdentity,
    call: Call,
    owned: Mutex<Owned>,
}

/// The tasks that the thread owns: its turns, and the task that copies the
/// call into the thread.
#[derive(Default)]
struct Owned {
    ended: bool,
    /// Aborted when the set drops with the thread.
    tasks: JoinSet<()>,
}

impl Thread {
    /// Must be called within a Tokio runtime.
    pub(crate) fn new(directory: Arc<Directory>, info: ThreadInfo, history: Vec<Update>) -> Self {
        let shared = Arc::new(Shared::new(history));
        let session = Session::new(
            CallTokenSource::new(Arc::clone(&directory), &info.id),
            directory.options.call_options.clone(),
        );
        let mut owned = Owned::default();
        owned.tasks.spawn(forward_call(session.subscribe(), session.state(), Arc::clone(&shared)));
        Self {
            user: ParticipantIdentity(directory.options.scope.clone()),
            agent: ParticipantIdentity(directory.options.agent_name.clone()),
            call: Call::new(session, Arc::downgrade(&shared)),
            shared,
            owned: Mutex::new(owned),
            directory,
            info,
        }
    }

    /// The thread id (`AT_...`).
    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// The thread's record as of when it was created or opened: its title
    /// and other attributes, and when it was last active. It is what a
    /// list shows for the thread; [`ThreadDirectory::get`] fetches the
    /// current record.
    ///
    /// [`ThreadDirectory::get`]: crate::ThreadDirectory::get
    pub fn info(&self) -> &ThreadInfo {
        &self.info
    }

    /// The message feed, ordered by [`Message::seq`].
    ///
    /// `seq` identifies a message in the feed: it is unique, and an update
    /// in place keeps it. `id` is unique only together with
    /// `participant_identity`.
    pub fn messages(&self) -> Vec<Message> {
        self.shared.messages()
    }

    /// A receiver of changes to the feed and of ended turns.
    ///
    /// Unbounded, like [`Session::subscribe`]. Drop the receiver to
    /// unsubscribe. The receiver ends when the thread ends.
    ///
    /// To show the feed and then follow it, use
    /// [`subscribe_with_messages`](Self::subscribe_with_messages), which
    /// takes both at once.
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<ThreadEvent> {
        self.shared.subscribe()
    }

    /// The feed and a receiver of its later changes, taken together: no
    /// change falls between the two, so the receiver does not replay what
    /// the messages already hold. Like [`messages`](Self::messages) and
    /// [`subscribe`](Self::subscribe) otherwise.
    pub fn subscribe_with_messages(&self) -> (Vec<Message>, mpsc::UnboundedReceiver<ThreadEvent>) {
        self.shared.subscribe_with_messages()
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
    ///
    /// Awaiting the acceptance is usually one round trip to the endpoint.
    /// Only the caller waits: the turn then runs in its own task, and the
    /// feed and the call stay live. Spawn the send if the caller must not
    /// block, e.g. the one loop of an actor.
    pub async fn send_text(&self, text: &str) -> Result<Message, SendError> {
        if lock(&self.owned).ended {
            return Err(SendError::Ended);
        }

        let thread_id = &self.info.id;
        let message_id = new_id("msg");
        let input_task = self.shared.input_task();
        let body = a2a::stream_request(
            &new_id("rpc"),
            &message_id,
            thread_id,
            input_task.as_deref(),
            text,
        );
        let token =
            self.directory.api_key.thread_token(&self.directory.options.scope, thread_id)?;
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
            while owned.tasks.try_join_next().is_some() {}
            owned.tasks.spawn(turn.run(frames));
        }
        Ok(message)
    }

    /// The thread's call. [`Call::start`] joins a room with the agent, and
    /// its state is [`ThreadState::call`].
    pub fn call(&self) -> &Call {
        &self.call
    }

    /// Ends the thread: stops its turns, ends its call and ends the feed.
    /// Later sends fail with [`SendError::Ended`], and the call does not
    /// start again.
    pub async fn end(&self) {
        let mut tasks = {
            let mut owned = lock(&self.owned);
            if owned.ended {
                return;
            }
            owned.ended = true;
            mem::take(&mut owned.tasks)
        };
        tasks.shutdown().await;
        self.call.close();
        self.call.end().await;
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
        // The tasks stop when their `JoinSet` drops with the thread.
        self.call.close();
        // The app may still hold the call, which keeps its session alive, so
        // end it. Ending is async and finishes on its own; without a runtime,
        // the session ends when the app drops its call.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let call = self.call.clone();
            runtime.spawn(async move { call.end().await });
        }
        self.shared.close();
    }
}

/// Copies the call's messages into the thread's feed and the call's state
/// into the thread's state, until the thread aborts this task.
async fn forward_call(
    mut events: mpsc::UnboundedReceiver<SessionEvent>,
    mut state: watch::Receiver<SessionState>,
    shared: Arc<Shared>,
) {
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Some(SessionEvent::MessageUpserted(message)) => {
                    shared.upsert(Update::from(message));
                }
                Some(_) => {}
                // The session is gone, which only happens with the thread.
                None => return,
            },
            changed = state.changed() => {
                if changed.is_err() {
                    return;
                }
                shared.sync_call(&state);
            }
        }
    }
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{:032x}", rand::random::<u128>())
}

#[cfg(test)]
mod tests {
    use super::*;

    use livekit_protocol::agent::agent_thread;

    use crate::{CallError, SessionOptions, ThreadDirectory, ThreadDirectoryOptions};

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
                call_options: SessionOptions::default(),
            },
        );
        let info = agent_thread::Thread { thread_id: "AT_test".into(), ..Default::default() };
        Thread::new(Arc::clone(&directory.inner), info.into(), Vec::new())
    }

    #[tokio::test]
    async fn the_thread_keeps_its_record() {
        let thread = thread();
        assert_eq!(thread.id(), "AT_test");
        assert_eq!(thread.info().id, "AT_test");
        assert_eq!(thread.info().title, None);
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
        let call = thread.call().clone();
        let shared = Arc::downgrade(&thread.shared);
        let mut events = thread.subscribe();
        let mut state = thread.state();

        drop(thread);
        settle().await;

        assert!(events.recv().await.is_none());
        // The forwarding task held the last other strong reference to the
        // feed: the call the app still holds only has a weak one.
        assert!(shared.upgrade().is_none());
        assert!(state.changed().await.is_err());
        assert!(matches!(call.start().await, Err(CallError::ThreadEnded)));
    }

    #[tokio::test]
    async fn ending_the_thread_stops_sends_and_calls() {
        let thread = thread();
        let call = thread.call().clone();
        let mut events = thread.subscribe();

        thread.end().await;

        assert!(events.recv().await.is_none());
        assert!(matches!(thread.send_text("Hi").await, Err(SendError::Ended)));
        assert!(matches!(call.start().await, Err(CallError::ThreadEnded)));
        assert!(matches!(thread.call().start().await, Err(CallError::ThreadEnded)));
        assert_eq!(*thread.state().borrow(), ThreadState::default());
        assert_eq!(Arc::strong_count(&thread.shared), 1);
    }
}
