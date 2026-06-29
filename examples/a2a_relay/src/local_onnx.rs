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

use log::{debug, info, warn};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use livekit_a2a_relay::{A2aClient, A2aFrame};
use sherpa_onnx::{
    GenerationConfig, OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTts,
    OfflineTtsConfig, OfflineTtsModelConfig, OfflineTtsVitsModelConfig, OfflineWhisperModelConfig,
};

/// Internal state for speech buffering and silence detection.
struct ClientState {
    buffer: Vec<i16>,
    speaking: bool,
    last_speech_time: Option<Instant>,
}

/// A local A2A Client that performs local Whisper STT and local Piper TTS.
/// Bridges a text-only agent to LiveKit's audio relay.
pub struct LocalOnnxA2aClient {
    agent_url: String,
    recognizer: Arc<Mutex<OfflineRecognizer>>,
    tts: Arc<Mutex<OfflineTts>>,
    frame_tx: mpsc::UnboundedSender<A2aFrame>,
    frame_rx: Mutex<Option<mpsc::UnboundedReceiver<A2aFrame>>>,
    state: Mutex<ClientState>,
    vad_threshold: f32,
}

impl LocalOnnxA2aClient {
    pub fn new(
        agent_url: String,
        model_dir: &str,
        vad_threshold: f32,
        stt_model: &str,
        tts_model: &str,
    ) -> Self {
        info!("Initializing local ONNX Speech Recognizer (Whisper {})...", stt_model);
        let model_path = Path::new(model_dir);

        // Configure Whisper
        let (whisper_dir_name, encoder_name, decoder_name, tokens_name) = match stt_model {
            "base" => (
                "sherpa-onnx-whisper-base.en",
                "base.en-encoder.int8.onnx",
                "base.en-decoder.int8.onnx",
                "base.en-tokens.txt",
            ),
            "small" => (
                "sherpa-onnx-whisper-small.en",
                "small.en-encoder.int8.onnx",
                "small.en-decoder.int8.onnx",
                "small.en-tokens.txt",
            ),
            _ => (
                "sherpa-onnx-whisper-tiny.en",
                "tiny.en-encoder.int8.onnx",
                "tiny.en-decoder.int8.onnx",
                "tiny.en-tokens.txt",
            ),
        };

        let whisper_dir = model_path.join(whisper_dir_name);
        let encoder_path = whisper_dir.join(encoder_name);
        let decoder_path = whisper_dir.join(decoder_name);
        let tokens_path = whisper_dir.join(tokens_name);

        if !encoder_path.exists() || !decoder_path.exists() || !tokens_path.exists() {
            panic!(
                "Whisper {} model files not found in {:?}. Please run `./scripts/download_onnx_models.sh --stt {}`",
                stt_model, whisper_dir, stt_model
            );
        }

        let mut whisper_config = OfflineWhisperModelConfig::default();
        whisper_config.encoder = Some(encoder_path.to_string_lossy().to_string());
        whisper_config.decoder = Some(decoder_path.to_string_lossy().to_string());
        whisper_config.language = Some("en".to_string());
        whisper_config.task = Some("transcribe".to_string());

        let mut recognizer_config = OfflineRecognizerConfig::default();
        recognizer_config.model_config = OfflineModelConfig {
            whisper: whisper_config,
            tokens: Some(tokens_path.to_string_lossy().to_string()),
            num_threads: 2,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        };

        let recognizer = OfflineRecognizer::create(&recognizer_config)
            .expect("Failed to create offline speech recognizer");

        info!("Initializing local ONNX Text-to-Speech (Piper {})...", tts_model);

        // Configure Piper
        let (piper_dir_name, piper_model_name, piper_tokens_name) = match tts_model {
            "high" => ("vits-piper-en_US-libritts-high", "en_US-libritts-high.onnx", "tokens.txt"),
            _ => ("vits-piper-en_US-lessac-medium", "en_US-lessac-medium.onnx", "tokens.txt"),
        };

        let piper_dir = model_path.join(piper_dir_name);
        let piper_model_path = piper_dir.join(piper_model_name);
        let piper_tokens_path = piper_dir.join(piper_tokens_name);
        let piper_data_dir = piper_dir.join("espeak-ng-data");

        if !piper_model_path.exists() || !piper_tokens_path.exists() || !piper_data_dir.exists() {
            panic!(
                "Piper {} model files not found in {:?}. Please run `./scripts/download_onnx_models.sh --tts {}`",
                tts_model, piper_dir, tts_model
            );
        }

        let vits_config = OfflineTtsVitsModelConfig {
            model: Some(piper_model_path.to_string_lossy().to_string()),
            lexicon: None,
            tokens: Some(piper_tokens_path.to_string_lossy().to_string()),
            data_dir: Some(piper_data_dir.to_string_lossy().to_string()),
            noise_scale: 0.667,
            noise_scale_w: 0.8,
            length_scale: 1.0,
            dict_dir: None,
        };

        let mut tts_config = OfflineTtsConfig::default();
        tts_config.model = OfflineTtsModelConfig {
            vits: vits_config,
            num_threads: 2,
            debug: false,
            ..Default::default()
        };

        let tts = OfflineTts::create(&tts_config).expect("Failed to create offline TTS");

        let (tx, rx) = mpsc::unbounded_channel();
        info!("Local ONNX A2A Client initialized successfully.");

        Self {
            agent_url,
            recognizer: Arc::new(Mutex::new(recognizer)),
            tts: Arc::new(Mutex::new(tts)),
            frame_tx: tx,
            frame_rx: Mutex::new(Some(rx)),
            state: Mutex::new(ClientState {
                buffer: Vec::new(),
                speaking: false,
                last_speech_time: None,
            }),
            vad_threshold,
        }
    }
}

impl A2aClient for LocalOnnxA2aClient {
    fn send_audio(
        &self,
        turn_id: u64,
        samples: &[i16],
    ) -> impl Future<Output = Result<(), String>> + Send {
        // Prepare variables outside the async block
        let mut trigger_turn = false;
        let mut audio_payload = Vec::new();

        if !samples.is_empty() {
            // 1. Simple Energy VAD calculation
            let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / samples.len() as f64).sqrt();
            let normalized = (rms / i16::MAX as f64) as f32;
            let is_speech = normalized > self.vad_threshold;

            let mut state = self.state.lock().unwrap();
            let now = Instant::now();

            if is_speech {
                if !state.speaking {
                    info!("VAD: Speech started (energy: {:.5})", normalized);
                }
                state.speaking = true;
                state.last_speech_time = Some(now);
                state.buffer.extend_from_slice(samples);
            } else if state.speaking {
                // User is currently speaking but paused/silent in this chunk.
                state.buffer.extend_from_slice(samples);

                if let Some(last_time) = state.last_speech_time {
                    // If silent for more than 800ms, consider the speech turn completed
                    if now.duration_since(last_time) > Duration::from_millis(800) {
                        state.speaking = false;
                        state.last_speech_time = None;
                        audio_payload = std::mem::take(&mut state.buffer);
                        trigger_turn = true;
                        info!(
                            "VAD: Silence detected. Triggering transcription on {} samples.",
                            audio_payload.len()
                        );
                    }
                }
            }
        }

        let agent_url = self.agent_url.clone();
        let frame_tx = self.frame_tx.clone();
        let recognizer = self.recognizer.clone();
        let tts = self.tts.clone();

        async move {
            if !trigger_turn || audio_payload.is_empty() {
                return Ok(());
            }

            // 1. STT (Whisper) on blocking thread pool
            let transcribed_text = tokio::task::spawn_blocking(move || {
                let f32_samples: Vec<f32> =
                    audio_payload.iter().map(|&s| s as f32 / 32768.0).collect();

                let recognizer_lock = recognizer.lock().unwrap();
                let stream = recognizer_lock.create_stream();
                stream.accept_waveform(16000, &f32_samples);
                recognizer_lock.decode(&stream);
                stream.get_result().map(|r| r.text).unwrap_or_default()
            })
            .await
            .map_err(|e| format!("STT task failed: {e}"))?;

            let trimmed_text = transcribed_text.trim();
            if trimmed_text.is_empty() {
                debug!("STT decoded empty text, skipping agent query.");
                return Ok(());
            }

            info!("STT (Whisper) decoded: \"{}\"", trimmed_text);

            let mut endpoint = agent_url.clone();
            if !endpoint.contains("/message:") {
                if endpoint.ends_with('/') {
                    endpoint.push_str("message:stream");
                } else {
                    endpoint.push_str("/message:stream");
                }
            }

            // 2. Query the text agent via HTTP POST to the A2A message:stream endpoint
            info!("Querying text agent at {}...", endpoint);
            let client = reqwest::Client::new();

            let message_id = uuid::Uuid::new_v4().to_string();
            let task_id = uuid::Uuid::new_v4().to_string();
            let context_id = uuid::Uuid::new_v4().to_string();

            let request_payload = serde_json::json!({
                "message": {
                    "role": "ROLE_USER",
                    "parts": [
                        {
                            "text": trimmed_text,
                            "mediaType": "text/plain"
                        }
                    ],
                    "messageId": message_id,
                    "taskId": task_id,
                    "contextId": context_id
                },
                "configuration": {
                    "acceptedOutputModes": ["text/plain"],
                    "returnImmediately": false
                }
            });

            let res = client
                .post(&endpoint)
                .header("A2A-Version", "1.0")
                .json(&request_payload)
                .send()
                .await
                .map_err(|e| format!("Failed to send request to text agent: {e}"))?;

            if !res.status().is_success() {
                return Err(format!("Text agent returned error status: {}", res.status()));
            }

            use futures_util::StreamExt;
            let mut stream = res.bytes_stream();
            let mut buffer = Vec::new();
            let mut reply_text = String::new();

            while let Some(chunk_res) = stream.next().await {
                let chunk = chunk_res.map_err(|e| format!("Failed to read stream chunk: {e}"))?;
                buffer.extend_from_slice(&chunk);

                while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=pos).collect::<Vec<u8>>();
                    let mut line = String::from_utf8_lossy(&line_bytes).into_owned();
                    if line.ends_with('\n') {
                        line.pop();
                    }
                    if line.ends_with('\r') {
                        line.pop();
                    }

                    if line.starts_with("data: ") {
                        let json_str = &line["data: ".len()..];
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                            let (speech, status) = extract_text_from_stream_response(&val);
                            if let Some(txt) = speech {
                                debug!("Agent reply chunk: {}", txt);
                                reply_text.push_str(&txt);
                            }
                            if let Some(txt) = status {
                                info!("Agent status update: {}", txt);
                            }
                        }
                    }
                }
            }

            let trimmed_reply = reply_text.trim().to_string();
            if trimmed_reply.is_empty() {
                warn!("Received empty reply from text agent.");
                return Ok(());
            }

            info!("Text Agent reply: \"{}\"", trimmed_reply);

            // 3. TTS (Piper) on blocking thread pool
            let synthesized_audio = tokio::task::spawn_blocking(move || {
                let tts_lock = tts.lock().unwrap();
                let gen_config = GenerationConfig { speed: 1.0, sid: 0, ..Default::default() };
                let audio = tts_lock.generate_with_config(
                    &trimmed_reply,
                    &gen_config,
                    None::<fn(&[f32], f32) -> bool>,
                );

                if let Some(generated) = audio {
                    generated
                        .samples()
                        .iter()
                        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                        .collect::<Vec<i16>>()
                } else {
                    Vec::new()
                }
            })
            .await
            .map_err(|e| format!("TTS task failed: {e}"))?;

            if synthesized_audio.is_empty() {
                warn!("TTS synthesized empty audio.");
                return Ok(());
            }

            info!(
                "TTS (Piper) synthesized {} audio samples. Feeding back to relay (turn={}).",
                synthesized_audio.len(),
                turn_id
            );

            // 4. Send frames to the relay actor
            let _ = frame_tx.send(A2aFrame { turn_id, samples: synthesized_audio });

            Ok(())
        }
    }

    fn cancel_turn(&self, _turn_id: u64) -> impl Future<Output = Result<(), String>> + Send {
        // User interrupted the agent's turn. The relay actor handles clearing
        // the playback jitter buffer and incrementing turn ID. We do not want
        // to clear the input audio buffer here, as that would discard the user's speech.
        async move { Ok(()) }
    }

    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move { Ok(()) }
    }

    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move { Ok(()) }
    }

    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame> {
        self.frame_rx
            .lock()
            .expect("frame_rx lock poisoned")
            .take()
            .expect("subscribe_frames called twice")
    }
}

/// Helper to parse SSE stream responses and separate speech response text from progress status updates.
fn extract_text_from_stream_response(val: &serde_json::Value) -> (Option<String>, Option<String>) {
    let mut speech_text = None;
    let mut status_text = None;

    // Check artifactUpdate (final answers)
    if let Some(artifact_update) = val.get("artifactUpdate").or_else(|| val.get("artifact_update"))
    {
        if let Some(artifact) = artifact_update.get("artifact") {
            if let Some(parts) = artifact.get("parts").and_then(|p| p.as_array()) {
                let mut out = String::new();
                for part in parts {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        out.push_str(text);
                    }
                }
                if !out.is_empty() {
                    speech_text = Some(out);
                }
            }
        }
    }
    // Check message (standard text message/chat reply)
    else if let Some(message) = val.get("message") {
        if let Some(parts) = message.get("parts").and_then(|p| p.as_array()) {
            let mut out = String::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    out.push_str(text);
                }
            }
            if !out.is_empty() {
                speech_text = Some(out);
            }
        }
    }
    // Check statusUpdate (reasoning/status progress logs)
    else if let Some(status_update) = val.get("statusUpdate").or_else(|| val.get("status_update"))
    {
        if let Some(status) = status_update.get("status") {
            if let Some(message) = status.get("message") {
                if let Some(parts) = message.get("parts").and_then(|p| p.as_array()) {
                    let mut out = String::new();
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            out.push_str(text);
                        }
                    }
                    if !out.is_empty() {
                        status_text = Some(out);
                    }
                }
            }
        }
    }

    (speech_text, status_text)
}
