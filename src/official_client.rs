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

//! Production [`A2aClient`] implementation backed by the upstream
//! [`a2a_client::A2AClient`] crate.
//!
//! # Architecture
//!
//! `OfficialA2aClient` owns a single **audio-streaming task** that runs for the
//! lifetime of the client.  The task calls
//! [`send_streaming_message`](a2a_client::A2AClient::send_streaming_message)
//! once per *turn*, forwards received [`Part::Raw`] bytes to the jitter buffer
//! via an unbounded channel, and restarts after each turn completes or is
//! cancelled.
//!
//! Outgoing user audio is accumulated in a bounded in-memory buffer and
//! flushed to the agent on each call to [`send_audio`].  In a future release
//! this will switch to a true bidirectional streaming transport (e.g.
//! SLIM-RPC) once the upstream crate stabilises that interface.

use std::future::Future;
use std::sync::{Arc, Mutex};

use a2a_client::types::v1::{
    self as a2a, Message, Part, Role, SendMessageConfiguration, SendMessageRequest, StreamResponse,
};
use a2a_client::A2AClient;
use futures_util::StreamExt;
use log::{debug, error, info, warn};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{A2aClient, A2aFrame};

/// Error returned when no audio parts are found in a streaming response chunk.
const MIME_AUDIO_PCM: &str = "audio/pcm;rate=16000";

/// Production [`A2aClient`] backed by the upstream `a2a-client` crate.
///
/// # Example
///
/// ```no_run
/// use livekit_a2a_relay::official_client::OfficialA2aClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let client = OfficialA2aClient::from_agent_url("https://my-agent.example.com").await?;
/// # Ok(())
/// # }
/// ```
pub struct OfficialA2aClient {
    inner: Arc<A2AClient>,
    /// Pending PCM samples to send on the next turn.
    pending_audio: Arc<Mutex<Vec<i16>>>,
    /// Channel used to push the current pending audio buffer to the streaming
    /// task and trigger a new agent turn.
    turn_tx: mpsc::UnboundedSender<(u64, Vec<i16>)>,
    /// Frames coming back from the agent, handed out via [`subscribe_frames`].
    frame_rx: Mutex<Option<mpsc::UnboundedReceiver<A2aFrame>>>,
    /// Signals the streaming task to cancel the currently active turn.
    cancel_tx: mpsc::UnboundedSender<u64>,
}

impl OfficialA2aClient {
    /// Create a new `OfficialA2aClient` by fetching the agent card from
    /// `agent_base_url` (e.g. `"https://my-agent.example.com"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the agent card cannot be fetched or the agent does
    /// not advertise a supported `JSONRPC` or `HTTP+JSON` endpoint.
    pub async fn from_agent_url(
        agent_base_url: impl AsRef<str>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let inner = A2AClient::from_card_url(agent_base_url).await?;
        Ok(Self::new(inner))
    }

    /// Create a new `OfficialA2aClient` from an already-constructed
    /// [`A2AClient`].
    pub fn new(inner: A2AClient) -> Self {
        let inner = Arc::new(inner);
        let pending_audio: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::new()));
        let (turn_tx, turn_rx) = mpsc::unbounded_channel::<(u64, Vec<i16>)>();
        let (frame_tx, frame_rx) = mpsc::unbounded_channel::<A2aFrame>();
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<u64>();

        let client_clone = inner.clone();
        tokio::spawn(stream_task(client_clone, turn_rx, cancel_rx, frame_tx));

        Self { inner, pending_audio, turn_tx, frame_rx: Mutex::new(Some(frame_rx)), cancel_tx }
    }

    /// Expose the underlying [`A2AClient`] for advanced use.
    pub fn inner(&self) -> &A2AClient {
        &self.inner
    }
}

impl A2aClient for OfficialA2aClient {
    /// Buffer the provided PCM samples locally and flush them to the remote
    /// agent as a streaming message turn.
    ///
    /// Each call accumulates samples until [`send_audio`] has gathered at
    /// least 20 ms worth of 16 kHz audio (320 samples), at which point the
    /// buffered payload is dispatched to the agent streaming task.
    fn send_audio(
        &self,
        turn_id: u64,
        samples: &[i16],
    ) -> impl Future<Output = Result<(), String>> + Send {
        // Drain the pending buffer and trigger a turn if we have accumulated
        // enough audio (≥ 320 samples ≈ 20 ms at 16 kHz).
        let mut pending = self.pending_audio.lock().expect("pending_audio lock poisoned");
        pending.extend_from_slice(samples);

        let should_flush = pending.len() >= 320;
        let payload = if should_flush {
            let p = std::mem::take(&mut *pending);
            Some((turn_id, p))
        } else {
            None
        };
        drop(pending);

        let turn_tx = self.turn_tx.clone();
        async move {
            if let Some((t_id, payload)) = payload {
                turn_tx
                    .send((t_id, payload))
                    .map_err(|e| format!("send_audio: turn channel closed: {e}"))?;
            }
            Ok(())
        }
    }

    /// Send a cancellation notice for the given turn to the streaming task.
    fn cancel_turn(&self, turn_id: u64) -> impl Future<Output = Result<(), String>> + Send {
        let cancel_tx = self.cancel_tx.clone();
        async move { cancel_tx.send(turn_id).map_err(|e| format!("cancel_turn: channel closed: {e}")) }
    }

    /// Request the speaking floor from the remote agent (no-op in this
    /// transport; floor management is handled by the agent itself via task
    /// state).
    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            debug!("OfficialA2aClient: floor requested (no-op for HTTP transport)");
            Ok(())
        }
    }

    /// Release the speaking floor (no-op in this transport).
    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            debug!("OfficialA2aClient: floor released (no-op for HTTP transport)");
            Ok(())
        }
    }

    /// Subscribe to incoming [`A2aFrame`]s from the remote agent.
    ///
    /// # Panics
    ///
    /// Panics if called more than once (each client supports a single
    /// subscriber, consistent with the relay actor model).
    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame> {
        self.frame_rx
            .lock()
            .expect("frame_rx lock poisoned")
            .take()
            .expect("subscribe_frames called more than once")
    }
}

// ---------------------------------------------------------------------------
// Internal streaming task
// ---------------------------------------------------------------------------

/// Long-lived background task that drives agent turns and forwards audio
/// frames to the relay actor.
///
/// Lifecycle:
/// 1. Wait for a batch of PCM samples on `turn_rx`.
/// 2. Build a [`SendMessageRequest`] with the audio encoded as a raw `Part`.
/// 3. Call `send_streaming_message` and stream the response.
/// 4. For each received [`Part::Raw`] with MIME `audio/pcm`, decode i16
///    samples and forward them to `frame_tx`.
/// 5. When a cancellation arrives on `cancel_rx`, abort the current SSE
///    stream and restart.
async fn stream_task(
    client: Arc<A2AClient>,
    mut turn_rx: mpsc::UnboundedReceiver<(u64, Vec<i16>)>,
    mut cancel_rx: mpsc::UnboundedReceiver<u64>,
    frame_tx: mpsc::UnboundedSender<A2aFrame>,
) {
    // Monotonically increasing turn counter shared with the relay actor via
    // cancel signals (the relay actor owns the TurnManager; we track ours
    // locally to tag outgoing frames).
    loop {
        // Wait for the next audio payload (a new turn begins).
        let (current_turn, pcm_samples) = match turn_rx.recv().await {
            Some(t) => t,
            None => {
                info!("OfficialA2aClient turn channel closed — stopping stream task");
                break;
            }
        };

        debug!(
            "OfficialA2aClient: starting turn {} with {} samples",
            current_turn,
            pcm_samples.len()
        );

        let request = build_send_message_request(&pcm_samples);

        let sse_stream = match client.send_streaming_message(request).await {
            Ok(s) => s,
            Err(e) => {
                error!("OfficialA2aClient: send_streaming_message failed: {e}");
                continue;
            }
        };

        // Pin the SSE stream so we can select! over it.
        tokio::pin!(sse_stream);

        loop {
            tokio::select! {
                biased;

                // Cancellation from the relay actor.
                Some(cancelled_turn) = cancel_rx.recv() => {
                    if cancelled_turn == current_turn {
                        debug!(
                            "OfficialA2aClient: cancel received for turn {} (current={}). Aborting stream.",
                            cancelled_turn, current_turn
                        );
                        // Drop the SSE stream (breaks the HTTP connection) and
                        // start the next turn.
                        break;
                    } else {
                        debug!(
                            "OfficialA2aClient: cancel received for outdated turn {} (current={}). Ignoring.",
                            cancelled_turn, current_turn
                        );
                    }
                }

                // Next SSE event from the agent.
                item = sse_stream.next() => {
                    match item {
                        None => {
                            debug!("OfficialA2aClient: SSE stream ended for turn {}", current_turn);
                            break;
                        }
                        Some(Err(e)) => {
                            warn!("OfficialA2aClient: SSE error on turn {}: {e}", current_turn);
                            break;
                        }
                        Some(Ok(stream_response)) => {
                            handle_stream_response(
                                stream_response,
                                current_turn,
                                &frame_tx,
                            );
                        }
                    }
                }
            }
        }
    }

    info!("OfficialA2aClient streaming task exited");
}

/// Build a [`SendMessageRequest`] that carries raw PCM audio as a `Part`.
fn build_send_message_request(pcm_samples: &[i16]) -> SendMessageRequest {
    // Encode i16 samples as little-endian bytes.
    let raw_bytes: Vec<u8> = pcm_samples.iter().flat_map(|s| s.to_le_bytes()).collect();

    let audio_part = Part {
        media_type: MIME_AUDIO_PCM.to_string(),
        content: Some(a2a::part::Content::Raw(raw_bytes)),
        ..Default::default()
    };

    let message = Message {
        message_id: Uuid::new_v4().to_string(),
        role: Role::User as i32,
        parts: vec![audio_part],
        ..Default::default()
    };

    SendMessageRequest {
        message: Some(message),
        configuration: Some(SendMessageConfiguration {
            accepted_output_modes: vec![MIME_AUDIO_PCM.to_string()],
            return_immediately: false,
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Extract audio frames from a single [`StreamResponse`] and forward them.
fn handle_stream_response(
    response: StreamResponse,
    turn_id: u64,
    frame_tx: &mpsc::UnboundedSender<A2aFrame>,
) {
    use a2a::stream_response::Payload;

    let payload = match response.payload {
        Some(p) => p,
        None => return,
    };

    // We only care about parts that carry raw audio; other payloads (task
    // status, artifact updates, text messages) are ignored for now.
    let parts: Vec<Part> = match payload {
        Payload::Message(msg) => msg.parts,
        Payload::Task(task) => {
            task.status.and_then(|s| s.message).map(|m| m.parts).unwrap_or_default()
        }
        Payload::ArtifactUpdate(ev) => ev.artifact.map(|a| a.parts).unwrap_or_default(),
        Payload::StatusUpdate(_) => return,
    };

    for part in parts {
        if part.media_type != MIME_AUDIO_PCM {
            continue;
        }
        let raw = match part.content {
            Some(a2a::part::Content::Raw(bytes)) => bytes,
            _ => continue,
        };

        // Decode little-endian i16 samples.
        if raw.len() % 2 != 0 {
            warn!("OfficialA2aClient: received odd-length audio payload ({} bytes)", raw.len());
            continue;
        }
        let samples: Vec<i16> =
            raw.chunks_exact(2).map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]])).collect();

        let frame = A2aFrame { turn_id, samples };
        if frame_tx.send(frame).is_err() {
            // Relay actor dropped its receiver — nothing to do.
            break;
        }
    }
}
