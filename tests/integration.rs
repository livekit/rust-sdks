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

//! Integration tests for `livekit-a2a-relay`.
//!
//! These tests exercise the relay components in isolation — no live LiveKit
//! server or A2A agent is required.
//!
//! Run with:
//!   cargo test -p livekit-a2a-relay --test integration

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use livekit_a2a_relay::{
    A2aClient, A2aFrame, AudioJitterBuffer, EnergyVad, TurnManager, VadDetector,
};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic test double for [`A2aClient`] that lets the test control
/// exactly which frames the relay actor receives.
struct TestA2aClient {
    frame_rx: Mutex<Option<mpsc::UnboundedReceiver<A2aFrame>>>,
    sent_samples: Arc<Mutex<Vec<Vec<i16>>>>,
    cancelled_turns: Arc<Mutex<Vec<u64>>>,
}

impl TestA2aClient {
    fn new() -> (Self, mpsc::UnboundedSender<A2aFrame>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client = Self {
            frame_rx: Mutex::new(Some(rx)),
            sent_samples: Arc::new(Mutex::new(Vec::new())),
            cancelled_turns: Arc::new(Mutex::new(Vec::new())),
        };
        (client, tx)
    }

    fn sent_samples(&self) -> Vec<Vec<i16>> {
        self.sent_samples.lock().unwrap().clone()
    }

    fn cancelled_turns(&self) -> Vec<u64> {
        self.cancelled_turns.lock().unwrap().clone()
    }
}

impl A2aClient for TestA2aClient {
    fn send_audio(
        &self,
        _turn_id: u64,
        samples: &[i16],
    ) -> impl Future<Output = Result<(), String>> + Send {
        let samples = samples.to_vec();
        let store = self.sent_samples.clone();
        async move {
            store.lock().unwrap().push(samples);
            Ok(())
        }
    }

    fn cancel_turn(&self, turn_id: u64) -> impl Future<Output = Result<(), String>> + Send {
        let store = self.cancelled_turns.clone();
        async move {
            store.lock().unwrap().push(turn_id);
            Ok(())
        }
    }

    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move { Ok(()) }
    }

    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move { Ok(()) }
    }

    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame> {
        self.frame_rx.lock().unwrap().take().expect("subscribe_frames called twice")
    }
}

// ---------------------------------------------------------------------------
// TurnManager tests
// ---------------------------------------------------------------------------

#[test]
fn turn_manager_increments_correctly() {
    let tm = TurnManager::new();
    assert_eq!(tm.current_turn(), 0);
    assert_eq!(tm.next_turn(), 1);
    assert_eq!(tm.next_turn(), 2);
    assert_eq!(tm.current_turn(), 2);
}

#[test]
fn turn_manager_validates_turn_ids() {
    let tm = TurnManager::new();
    assert!(tm.is_valid(0)); // initial turn
    tm.next_turn();
    assert!(!tm.is_valid(0));
    assert!(tm.is_valid(1));
}

// ---------------------------------------------------------------------------
// AudioJitterBuffer tests
// ---------------------------------------------------------------------------

#[test]
fn jitter_buffer_overflow_drops_oldest_samples() {
    // max_depth_ms=10ms at 16000 Hz => 160 samples max
    let mut buf = AudioJitterBuffer::new(5, 10, 16000);
    let samples = vec![1i16; 160];
    buf.push(&samples);
    assert_eq!(buf.len(), 160);

    // Push another 160 — should overflow, keeping only the newest 160
    let samples2 = vec![2i16; 160];
    buf.push(&samples2);
    assert_eq!(buf.len(), 160);

    let mut out = vec![0i16; 160];
    buf.pop(&mut out);
    // All popped samples should be from the second push (value 2)
    assert!(out.iter().all(|&s| s == 2), "expected all 2s after overflow: {:?}", &out[..5]);
}

#[test]
fn jitter_buffer_pop_pads_with_silence() {
    let mut buf = AudioJitterBuffer::new(20, 100, 16000);
    buf.push(&[100i16; 80]);

    let mut out = vec![99i16; 160]; // non-zero sentinel
    let read = buf.pop(&mut out);
    assert_eq!(read, 80);
    // First 80 samples are real data
    assert!(out[..80].iter().all(|&s| s == 100));
    // Remaining 80 samples are comfort noise padding
    assert!(out[80..].iter().all(|&s| s >= -16 && s <= 15));
}

#[test]
fn jitter_buffer_backpressure_triggers_above_target() {
    // target = 5ms @ 16kHz = 80 samples
    let mut buf = AudioJitterBuffer::new(5, 200, 16000);
    assert!(!buf.needs_backpressure());
    buf.push(&vec![0i16; 81]);
    assert!(buf.needs_backpressure());
}

// ---------------------------------------------------------------------------
// EnergyVad tests
// ---------------------------------------------------------------------------

#[test]
fn energy_vad_detects_loud_speech() {
    let mut vad = EnergyVad::new(0.01, 200, 16000);
    assert!(!vad.process_samples(&vec![0i16; 160]));
    assert!(vad.process_samples(&vec![10000i16; 160]));
}

#[test]
fn energy_vad_hangover_keeps_active() {
    let mut vad = EnergyVad::new(0.01, 500, 16000); // 500 ms hangover
                                                    // Trigger speech
    assert!(vad.process_samples(&vec![10000i16; 160]));
    // Immediately go silent — should still be active within hangover window
    assert!(vad.process_samples(&vec![0i16; 160]));
}

// ---------------------------------------------------------------------------
// TestA2aClient double behaviour
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_client_records_sent_audio() {
    let (client, _tx) = TestA2aClient::new();
    let samples = vec![1000i16; 320];
    client.send_audio(1, &samples).await.unwrap();
    client.send_audio(1, &samples).await.unwrap();
    assert_eq!(client.sent_samples().len(), 2);
}

#[tokio::test]
async fn test_client_records_cancelled_turns() {
    let (client, _tx) = TestA2aClient::new();
    client.cancel_turn(1).await.unwrap();
    client.cancel_turn(5).await.unwrap();
    assert_eq!(client.cancelled_turns(), vec![1, 5]);
}

// ---------------------------------------------------------------------------
// Frame routing via jitter buffer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn frames_for_current_turn_are_buffered() {
    let jitter = Arc::new(Mutex::new(AudioJitterBuffer::new(60, 200, 16000)));
    let turn_mgr = Arc::new(TurnManager::new());

    let (_, frame_tx) = TestA2aClient::new();

    // Simulate what the relay actor's a2a_listener_task does
    let (tx, mut rx) = mpsc::unbounded_channel::<A2aFrame>();
    let jitter_clone = jitter.clone();
    let tm_clone = turn_mgr.clone();

    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if tm_clone.is_valid(frame.turn_id) {
                jitter_clone.lock().unwrap().push(&frame.samples);
            }
        }
    });

    // Send valid frame for turn 0
    tx.send(A2aFrame { turn_id: 0, samples: vec![1000i16; 160] }).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(jitter.lock().unwrap().len(), 160, "frame should have been buffered");

    // Advance turn and send stale frame for old turn
    turn_mgr.next_turn();
    tx.send(A2aFrame { turn_id: 0, samples: vec![2000i16; 160] }).unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    // Buffer should still be at 160 (stale frame dropped)
    assert_eq!(jitter.lock().unwrap().len(), 160, "stale frame should be dropped");

    let _ = frame_tx; // keep alive
}

// ---------------------------------------------------------------------------
// OfficialA2aClient unit test (requires `a2a-client` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "a2a-client")]
mod official_client_tests {
    use livekit_a2a_relay::official_client::OfficialA2aClient;
    use livekit_a2a_relay::{A2aClient, A2aFrame};
    use std::time::Duration;
    use tokio::time::timeout;
    use uuid::Uuid;

    /// Starts a minimal in-process A2A HTTP server that returns one synthetic
    /// audio chunk and verifies the OfficialA2aClient routes it through to
    /// `subscribe_frames`.
    ///
    /// This test requires the `axum` crate in dev-dependencies; see
    /// `Cargo.toml [dev-dependencies]`.
    #[tokio::test]
    async fn official_client_routes_audio_from_sse_stream() {
        // Start the in-process mock agent (same logic as `a2a_mock_agent` binary)
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let agent_url = format!("http://127.0.0.1:{port}");

        let app = build_mock_agent_router(&agent_url);
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a moment to bind
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Create the real client
        let client =
            OfficialA2aClient::from_agent_url(&agent_url).await.expect("Failed to create client");

        let mut frame_rx = client.subscribe_frames();

        // Push 320 samples → triggers a turn flush
        let samples = vec![1000i16; 320];
        client.send_audio(1, &samples).await.unwrap();

        // Expect at least one A2aFrame within 2 seconds
        let frame = timeout(Duration::from_secs(2), frame_rx.recv())
            .await
            .expect("timed out waiting for audio frame")
            .expect("frame channel closed unexpectedly");

        assert!(!frame.samples.is_empty(), "received frame must have samples");
    }

    fn build_mock_agent_router(base_url: &str) -> axum::Router {
        use axum::{
            routing::{get, post},
            Router,
        };

        let base_url = base_url.to_string();
        // Pass the base_url as state
        Router::new()
            .route("/.well-known/agent.json", get(test_agent_card))
            .route("/.well-known/agent-card.json", get(test_agent_card))
            .route("/message:stream", post(test_stream_msg))
            .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
            .with_state(base_url)
    }

    async fn test_agent_card(
        axum::extract::State(base_url): axum::extract::State<String>,
    ) -> axum::response::Json<serde_json::Value> {
        axum::response::Json(serde_json::json!({
            "name": "TestMockAgent",
            "version": "0.1.0",
            "supportedInterfaces": [
                {
                    "url": base_url,
                    "protocolBinding": "HTTP+JSON",
                    "protocolVersion": "0.3"
                }
            ],
            "capabilities": { "streaming": true },
            "defaultInputModes": ["audio/pcm;rate=16000"],
            "defaultOutputModes": ["audio/pcm;rate=16000"],
            "skills": []
        }))
    }

    async fn test_stream_msg(
        _body: axum::extract::Json<serde_json::Value>,
    ) -> axum::response::Response<axum::body::Body> {
        use axum::{
            body::Body,
            http::{header, HeaderMap, HeaderValue, StatusCode},
        };

        let task_id = Uuid::new_v4().to_string();
        let ctx_id = Uuid::new_v4().to_string();
        let msg_id = Uuid::new_v4().to_string();

        let samples: Vec<i16> = vec![4000i16; 1600];
        let raw: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let b64 = base64_simple(&raw);

        let evt = serde_json::json!({
            "message": {
                "messageId": msg_id,
                "contextId": ctx_id,
                "taskId": task_id,
                "role": "ROLE_AGENT",
                "parts": [{ "mediaType": "audio/pcm;rate=16000", "raw": b64 }]
            }
        });

        let body = Body::from(format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));

        let mut resp = axum::response::Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        *resp.headers_mut() = headers;
        resp
    }

    fn base64_simple(data: &[u8]) -> String {
        const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
            let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(C[((n >> 18) & 63) as usize] as char);
            out.push(C[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 { C[((n >> 6) & 63) as usize] as char } else { '=' });
            out.push(if chunk.len() > 2 { C[(n & 63) as usize] as char } else { '=' });
        }
        out
    }
}
