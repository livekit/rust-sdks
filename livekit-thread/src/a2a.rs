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

//! The A2A wire to an agent's text endpoint: one JSON-RPC request over HTTP,
//! answered by a stream of server-sent events, one frame each.

use std::collections::VecDeque;
use std::sync::Arc;

use livekit_net::{Header, HttpBodyStream, HttpMethod};
use serde::Deserialize;
use serde_json::json;

use crate::error::SendError;

/// The body of a `message/stream` request: `text` from the user in the thread
/// `context_id`. `task_id` continues a task that waits for input.
pub(crate) fn stream_request(
    request_id: &str,
    message_id: &str,
    context_id: &str,
    task_id: Option<&str>,
    text: &str,
) -> Vec<u8> {
    let mut message = json!({
        "messageId": message_id,
        "role": "user",
        "contextId": context_id,
        "parts": [{ "kind": "text", "text": text }],
    });
    if let Some(task_id) = task_id {
        message["taskId"] = task_id.into();
    }
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "message/stream",
        "params": { "message": message },
    })
    .to_string()
    .into_bytes()
}

/// Posts `body` to the endpoint at `url` and returns its frames once the
/// status and headers have arrived.
pub(crate) async fn open(url: &str, token: &str, body: Vec<u8>) -> Result<Frames, SendError> {
    let client = livekit_net::http_stream_client().ok_or(SendError::TransportNotConfigured)?;
    let headers = vec![
        header("Content-Type", "application/json"),
        header("Accept", "text/event-stream"),
        header("Authorization", &format!("Bearer {token}")),
    ];
    let response =
        client.request_stream(HttpMethod::Post, url.to_owned(), headers, Some(body)).await?;

    if !(200..300).contains(&response.status) {
        let mut body = Vec::new();
        while let Ok(Some(chunk)) = response.body.recv().await {
            body.extend(chunk);
        }
        return Err(SendError::Status {
            status: response.status,
            body: String::from_utf8_lossy(&body).into_owned(),
        });
    }

    let event_stream = response.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("content-type")
            && header.value.starts_with("text/event-stream")
    });
    Ok(Frames {
        body: response.body,
        event_stream,
        buffer: Vec::new(),
        ready: VecDeque::new(),
        done: false,
    })
}

fn header(name: &str, value: &str) -> Header {
    Header { name: name.to_owned(), value: value.to_owned() }
}

/// The frames of one response. An event stream gives one frame per event; a
/// plain JSON response gives its whole body as one frame.
pub(crate) struct Frames {
    body: Arc<dyn HttpBodyStream>,
    event_stream: bool,
    buffer: Vec<u8>,
    ready: VecDeque<String>,
    done: bool,
}

impl Frames {
    /// The next frame, or `None` when the response is complete.
    pub(crate) async fn next(&mut self) -> Result<Option<Frame>, SendError> {
        loop {
            if let Some(data) = self.ready.pop_front() {
                return Ok(Some(serde_json::from_str(&data)?));
            }
            if self.done {
                return Ok(None);
            }
            match self.body.recv().await? {
                Some(chunk) => {
                    // A JSON payload has no raw carriage return, so removing
                    // them all turns CRLF line ends into LF.
                    self.buffer.extend(chunk.into_iter().filter(|byte| *byte != b'\r'));
                    if self.event_stream {
                        self.ready.extend(cut_events(&mut self.buffer));
                    }
                }
                None => {
                    self.done = true;
                    if self.event_stream {
                        // An event that the server did not end with a blank line.
                        self.buffer.extend(b"\n\n");
                        self.ready.extend(cut_events(&mut self.buffer));
                    } else {
                        let body = String::from_utf8_lossy(&self.buffer).trim().to_owned();
                        if !body.is_empty() {
                            self.ready.push_back(body);
                        }
                    }
                }
            }
        }
    }
}

/// Removes every complete event from `buffer` and returns the data of each.
/// The incomplete rest stays in `buffer`.
fn cut_events(buffer: &mut Vec<u8>) -> Vec<String> {
    let mut events = Vec::new();
    while let Some(end) = buffer.windows(2).position(|window| window == b"\n\n") {
        let event: Vec<u8> = buffer.drain(..end + 2).collect();
        let event = String::from_utf8_lossy(&event[..end]);
        let data: Vec<&str> = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(|data| data.strip_prefix(' ').unwrap_or(data))
            .collect();
        if !data.is_empty() {
            events.push(data.join("\n"));
        }
    }
    events
}

/// One JSON-RPC response frame: a result or an error.
#[derive(Debug, Deserialize)]
pub(crate) struct Frame {
    pub result: Option<FrameResult>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcError {
    pub code: i64,
    #[serde(default, rename = "message")]
    pub reason: String,
}

/// What one frame carries. A stream sends each kind in its own frame.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrameResult {
    pub task: Option<Task>,
    pub status_update: Option<StatusUpdate>,
    pub artifact_update: Option<ArtifactUpdate>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Task {
    pub id: String,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Status {
    #[serde(default)]
    pub state: String,
    pub message: Option<WireMessage>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WireMessage {
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Part {
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusUpdate {
    pub task_id: String,
    #[serde(default)]
    pub status: Status,
    #[serde(default, rename = "final")]
    pub is_final: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactUpdate {
    pub task_id: String,
    #[serde(default)]
    pub artifact: Artifact,
    #[serde(default)]
    pub append: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Artifact {
    #[serde(default)]
    pub parts: Vec<Part>,
}

/// The text parts of `parts`, joined.
pub(crate) fn text_of(parts: &[Part]) -> String {
    parts.iter().filter_map(|part| part.text.as_deref()).collect()
}

/// The state of an A2A task, from its 1.0 (`TASK_STATE_INPUT_REQUIRED`) or
/// 0.3 (`input-required`) name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
    AuthRequired,
    Unknown,
}

impl TaskState {
    pub(crate) fn parse(name: &str) -> Self {
        let name = name.strip_prefix("TASK_STATE_").unwrap_or(name);
        match name.to_ascii_lowercase().replace('_', "-").as_str() {
            "submitted" => Self::Submitted,
            "working" => Self::Working,
            "input-required" => Self::InputRequired,
            "completed" => Self::Completed,
            "canceled" | "cancelled" => Self::Canceled,
            "failed" => Self::Failed,
            "rejected" => Self::Rejected,
            "auth-required" => Self::AuthRequired,
            _ => Self::Unknown,
        }
    }

    /// Whether the turn is over: the task ended or waits for the user.
    pub(crate) fn ends_turn(self) -> bool {
        !matches!(self, Self::Submitted | Self::Working | Self::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_events_keeps_the_incomplete_rest() {
        let mut buffer = b"data: {\"a\":1}\n\nevent: x\ndata: {\"b\":\n".to_vec();
        assert_eq!(cut_events(&mut buffer), ["{\"a\":1}"]);
        assert_eq!(buffer, b"event: x\ndata: {\"b\":\n");

        buffer.extend(b"data: 2}\n\n");
        assert_eq!(cut_events(&mut buffer), ["{\"b\":\n2}"]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn cut_events_skips_events_without_data() {
        let mut buffer = b": keep-alive\n\nid: 7\ndata:{}\n\n".to_vec();
        assert_eq!(cut_events(&mut buffer), ["{}"]);
    }

    #[test]
    fn task_state_reads_both_protocol_versions() {
        assert_eq!(TaskState::parse("TASK_STATE_INPUT_REQUIRED"), TaskState::InputRequired);
        assert_eq!(TaskState::parse("input-required"), TaskState::InputRequired);
        assert_eq!(TaskState::parse("TASK_STATE_COMPLETED"), TaskState::Completed);
        assert_eq!(TaskState::parse("something-new"), TaskState::Unknown);
        assert!(!TaskState::Working.ends_turn());
        assert!(TaskState::Canceled.ends_turn());
    }

    #[test]
    fn frames_parse_the_three_update_kinds() {
        let task: Frame = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":"1","result":{"task":{"id":"t","contextId":"AT_1",
                "status":{"state":"TASK_STATE_SUBMITTED"}}}}"#,
        )
        .unwrap();
        assert_eq!(task.result.unwrap().task.unwrap().id, "t");

        let artifact: Frame = serde_json::from_str(
            r#"{"result":{"artifactUpdate":{"taskId":"t","append":true,
                "artifact":{"artifactId":"answer","parts":[{"text":"Hel"},{"text":"lo"}]}}}}"#,
        )
        .unwrap();
        let update = artifact.result.unwrap().artifact_update.unwrap();
        assert!(update.append);
        assert_eq!(text_of(&update.artifact.parts), "Hello");

        let status: Frame = serde_json::from_str(
            r#"{"result":{"statusUpdate":{"taskId":"t","final":true,
                "status":{"state":"TASK_STATE_COMPLETED"}}}}"#,
        )
        .unwrap();
        assert!(status.result.unwrap().status_update.unwrap().is_final);

        let error: Frame =
            serde_json::from_str(r#"{"error":{"code":-32001,"message":"no task"}}"#).unwrap();
        assert_eq!(error.error.unwrap().reason, "no task");
    }

    #[test]
    fn stream_request_continues_a_task_only_when_given() {
        let body: serde_json::Value =
            serde_json::from_slice(&stream_request("1", "m", "AT_1", None, "Hi")).unwrap();
        assert_eq!(body["method"], "message/stream");
        assert_eq!(body["params"]["message"]["contextId"], "AT_1");
        assert!(body["params"]["message"].get("taskId").is_none());

        let body: serde_json::Value =
            serde_json::from_slice(&stream_request("1", "m", "AT_1", Some("t"), "Hi")).unwrap();
        assert_eq!(body["params"]["message"]["taskId"], "t");
    }
}
