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

//! One turn of the agent: the frames of one `message/stream` response, into
//! the feed as one growing answer.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use livekit::prelude::ParticipantIdentity;
use livekit_session::MessageOrigin;

use crate::a2a::{self, Frame, Frames, TaskState, WireMessage};
use crate::feed::{Shared, Update};
use crate::thread::TurnOutcome;

pub(crate) struct Turn {
    shared: Arc<Shared>,
    agent: ParticipantIdentity,
    started: DateTime<Utc>,
    /// The task of the turn. The answer message has this id.
    task_id: Option<String>,
    answer: String,
}

impl Turn {
    pub(crate) fn new(shared: Arc<Shared>, agent: ParticipantIdentity) -> Self {
        Self { shared, agent, started: Utc::now(), task_id: None, answer: String::new() }
    }

    /// Follows the remaining frames until the turn ends, then ends it.
    pub(crate) async fn run(mut self, mut frames: Frames) {
        let outcome = loop {
            match frames.next().await {
                Ok(Some(frame)) => {
                    if let Some(outcome) = self.apply(frame) {
                        break outcome;
                    }
                }
                Ok(None) => {
                    break TurnOutcome::Failed {
                        reason: "the stream ended before the turn finished".into(),
                    }
                }
                Err(error) => break TurnOutcome::Failed { reason: error.to_string() },
            }
        };
        self.end(outcome);
    }

    /// Ends the turn with `outcome`.
    pub(crate) fn end(self, outcome: TurnOutcome) {
        self.shared.end_turn(outcome, self.task_id);
    }

    /// Applies one frame to the feed. Returns the outcome when the turn ended.
    pub(crate) fn apply(&mut self, frame: Frame) -> Option<TurnOutcome> {
        if let Some(error) = frame.error {
            return Some(self.finish(TaskState::Failed, None, Some(error.reason)));
        }
        let result = frame.result?;

        if let Some(task) = result.task {
            self.task_id = Some(task.id);
            let state = TaskState::parse(&task.status.state);
            if state.ends_turn() {
                // A task that already ended carries its answer as artifacts.
                let answer: String =
                    task.artifacts.iter().map(|artifact| a2a::text_of(&artifact.parts)).collect();
                if !answer.is_empty() {
                    self.answer = answer;
                }
                return Some(self.finish(state, task.status.message.as_ref(), None));
            }
        }

        if let Some(update) = result.artifact_update {
            self.task_id.get_or_insert(update.task_id);
            let chunk = a2a::text_of(&update.artifact.parts);
            if update.append {
                self.answer.push_str(&chunk);
            } else {
                self.answer = chunk;
            }
            self.publish(false);
        }

        if let Some(update) = result.status_update {
            self.task_id.get_or_insert(update.task_id);
            let state = TaskState::parse(&update.status.state);
            if update.is_final || state.ends_turn() {
                return Some(self.finish(state, update.status.message.as_ref(), None));
            }
        }
        None
    }

    /// Completes the answer in the feed and maps the final state to the
    /// turn's outcome.
    fn finish(
        &mut self,
        state: TaskState,
        message: Option<&WireMessage>,
        error: Option<String>,
    ) -> TurnOutcome {
        let status_text = message.map(|message| a2a::text_of(&message.parts)).unwrap_or_default();
        let outcome = match state {
            TaskState::Completed | TaskState::Submitted | TaskState::Working => {
                TurnOutcome::Completed
            }
            TaskState::InputRequired => {
                // The agent's question rides on the status, not on an artifact.
                if !status_text.is_empty() {
                    if !self.answer.is_empty() {
                        self.answer.push_str("\n\n");
                    }
                    self.answer.push_str(&status_text);
                }
                TurnOutcome::InputRequired
            }
            TaskState::Canceled => TurnOutcome::Canceled,
            TaskState::Failed
            | TaskState::Rejected
            | TaskState::AuthRequired
            | TaskState::Unknown => {
                let reason = error
                    .or_else(|| (!status_text.is_empty()).then_some(status_text))
                    .unwrap_or_else(|| format!("the task ended as {state:?}"));
                TurnOutcome::Failed { reason }
            }
        };
        if !self.answer.is_empty() {
            self.publish(true);
        }
        outcome
    }

    fn publish(&self, is_final: bool) {
        let Some(task_id) = &self.task_id else {
            return;
        };
        self.shared.upsert(Update {
            id: task_id.clone(),
            timestamp: self.started,
            origin: MessageOrigin::AgentText,
            participant_identity: self.agent.clone(),
            text: self.answer.clone(),
            is_final,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(json: &str) -> Frame {
        serde_json::from_str(json).unwrap()
    }

    fn turn() -> (Arc<Shared>, Turn) {
        let shared = Arc::new(Shared::new(Vec::new()));
        let turn = Turn::new(Arc::clone(&shared), ParticipantIdentity("agent".into()));
        (shared, turn)
    }

    #[test]
    fn streamed_answer_grows_and_completes() {
        let (shared, mut turn) = turn();
        let frames = [
            r#"{"result":{"task":{"id":"t","status":{"state":"TASK_STATE_SUBMITTED"}}}}"#,
            r#"{"result":{"statusUpdate":{"taskId":"t","status":{"state":"TASK_STATE_WORKING"}}}}"#,
            r#"{"result":{"artifactUpdate":{"taskId":"t","artifact":{"parts":[{"text":"Hel"}]}}}}"#,
            r#"{"result":{"artifactUpdate":{"taskId":"t","append":true,"artifact":{"parts":[{"text":"lo"}]}}}}"#,
        ];
        for json in frames {
            assert_eq!(turn.apply(frame(json)), None);
        }
        let messages = shared.messages();
        assert_eq!((messages[0].text.as_str(), messages[0].is_final), ("Hello", false));

        let done = r#"{"result":{"statusUpdate":{"taskId":"t","final":true,"status":{"state":"TASK_STATE_COMPLETED"}}}}"#;
        assert_eq!(turn.apply(frame(done)), Some(TurnOutcome::Completed));
        let messages = shared.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!((messages[0].id.as_str(), messages[0].is_final), ("t", true));
    }

    #[test]
    fn input_required_shows_the_question() {
        let (shared, mut turn) = turn();
        let ask = r#"{"result":{"statusUpdate":{"taskId":"t","final":true,"status":{"state":"TASK_STATE_INPUT_REQUIRED",
            "message":{"role":"ROLE_AGENT","parts":[{"text":"Which day?"}]}}}}}"#;
        assert_eq!(turn.apply(frame(ask)), Some(TurnOutcome::InputRequired));
        assert_eq!(shared.messages()[0].text, "Which day?");
        turn.end(TurnOutcome::InputRequired);
        assert_eq!(shared.input_task().as_deref(), Some("t"));
    }

    #[test]
    fn rpc_error_fails_the_turn() {
        let (_, mut turn) = turn();
        let outcome = turn.apply(frame(r#"{"error":{"code":-32603,"message":"boom"}}"#));
        assert_eq!(outcome, Some(TurnOutcome::Failed { reason: "boom".into() }));
    }
}
