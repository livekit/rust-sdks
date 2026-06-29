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

//! Ultra-low latency Agent-to-Agent (A2A) relay for LiveKit.
//!
//! This crate provides the core building blocks for bridging a LiveKit room
//! with an A2A-compliant agent:
//!
//! - [`RelayActor`] — the main actor that coordinates WebRTC audio I/O with
//!   the A2A transport.
//! - [`TurnManager`] — atomic turn counter used to discard stale audio frames
//!   after an interruption.
//! - [`AudioJitterBuffer`] — ring buffer that smooths clock drift between the
//!   remote TTS generator and local WebRTC playback.
//! - [`EnergyVad`] — lightweight energy-based Voice Activity Detector.
//! - [`A2aClient`] / [`A2aFrame`] — trait and frame type for pluggable A2A
//!   transports.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use tokio::sync::{mpsc, watch};

use livekit::prelude::LocalAudioTrack;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::prelude::AudioFrame;
use livekit::Room;

// Re-export the official A2A client when the feature is enabled.
#[cfg(feature = "a2a-client")]
pub mod official_client;

#[cfg(feature = "a2a-client")]
pub use official_client::OfficialA2aClient;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single chunk of PCM audio received from (or destined for) an A2A agent.
pub struct A2aFrame {
    /// Conversational turn this frame belongs to.
    pub turn_id: u64,
    /// 16-bit signed PCM samples at the negotiated sample rate.
    pub samples: Vec<i16>,
}

/// Trait abstracting the A2A transport layer.
///
/// Implementations are responsible for serialising audio to a remote agent and
/// delivering response frames back via [`subscribe_frames`](Self::subscribe_frames).
pub trait A2aClient: Send + Sync + 'static {
    /// Send a buffer of PCM samples to the remote agent for the given turn.
    fn send_audio(
        &self,
        turn_id: u64,
        samples: &[i16],
    ) -> impl Future<Output = Result<(), String>> + Send;

    /// Cancel the specified turn (e.g. due to user interruption).
    fn cancel_turn(&self, turn_id: u64) -> impl Future<Output = Result<(), String>> + Send;

    /// Request the speaking floor from the remote agent.
    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// Release the speaking floor.
    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// Subscribe to incoming [`A2aFrame`]s from the agent.
    ///
    /// Must only be called once per client instance.
    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame>;
}

/// Trait for voice-activity detectors used by [`RelayActor`].
pub trait VadDetector: Send + 'static {
    /// Process a chunk of PCM samples and return `true` if speech is detected.
    fn process_samples(&mut self, samples: &[i16]) -> bool;
}

// ---------------------------------------------------------------------------
// TurnManager
// ---------------------------------------------------------------------------

/// Atomic turn counter that gates stale-frame rejection.
///
/// Every time the user interrupts, [`next_turn`](Self::next_turn) is called to
/// bump the counter.  Incoming agent frames whose `turn_id` does not match
/// [`current_turn`](Self::current_turn) are silently discarded.
pub struct TurnManager {
    current_turn: AtomicU64,
}

impl TurnManager {
    /// Create a new [`TurnManager`] starting at turn 0.
    pub fn new() -> Self {
        Self { current_turn: AtomicU64::new(0) }
    }

    /// Advance to the next turn and return the new turn id.
    pub fn next_turn(&self) -> u64 {
        self.current_turn.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Return the current turn id.
    pub fn current_turn(&self) -> u64 {
        self.current_turn.load(Ordering::SeqCst)
    }

    /// Return `true` if `packet_turn` matches the current turn.
    pub fn is_valid(&self, packet_turn: u64) -> bool {
        packet_turn == self.current_turn()
    }
}

impl Default for TurnManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AudioJitterBuffer
// ---------------------------------------------------------------------------

/// Ring buffer that absorbs clock-drift jitter between the remote TTS
/// generator and local WebRTC playback.
///
/// # Overflow
///
/// When the buffer exceeds `max_depth` samples the oldest samples are dropped
/// so that only the newest `max_depth` samples remain.
///
/// # Underflow
///
/// When [`pop`](Self::pop) is called but there are fewer buffered samples than
/// requested, the remaining output slots are filled with low-amplitude comfort
/// noise (values in `[-16, 15]`).
pub struct AudioJitterBuffer {
    buffer: VecDeque<i16>,
    target_depth: usize,
    max_depth: usize,
}

impl AudioJitterBuffer {
    /// Create a new buffer.
    ///
    /// * `target_depth_ms` — ideal fill level in milliseconds.
    /// * `max_depth_ms` — hard upper bound; overflow beyond this is dropped.
    /// * `sample_rate` — sample rate in Hz (e.g. 16 000).
    pub fn new(target_depth_ms: u32, max_depth_ms: u32, sample_rate: u32) -> Self {
        let target_depth = (target_depth_ms as usize * sample_rate as usize) / 1000;
        let max_depth = (max_depth_ms as usize * sample_rate as usize) / 1000;
        Self { buffer: VecDeque::with_capacity(max_depth), target_depth, max_depth }
    }

    /// Push samples into the buffer, dropping the oldest on overflow.
    pub fn push(&mut self, data: &[i16]) {
        self.buffer.extend(data);
        if self.buffer.len() > self.max_depth {
            let excess = self.buffer.len() - self.max_depth;
            self.buffer.drain(..excess);
        }
    }

    /// Pop up to `output.len()` samples.
    ///
    /// Returns the number of *real* (non-padding) samples written.  Any
    /// remaining slots are filled with comfort noise.
    pub fn pop(&mut self, output: &mut [i16]) -> usize {
        let read_len = std::cmp::min(self.buffer.len(), output.len());
        for (i, sample) in self.buffer.drain(..read_len).enumerate() {
            output[i] = sample;
        }
        // Fill remainder with comfort noise in [-16, 15].
        for i in read_len..output.len() {
            output[i] = comfort_noise_sample(i);
        }
        read_len
    }

    /// Number of samples currently buffered.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns `true` when the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Returns `true` when the buffer level exceeds the target depth,
    /// signalling that the producer should throttle.
    pub fn needs_backpressure(&self) -> bool {
        self.buffer.len() > self.target_depth
    }

    /// Discard all buffered samples (used on interruption).
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Deterministic low-amplitude comfort-noise sample in `[-16, 15]`.
fn comfort_noise_sample(index: usize) -> i16 {
    // Knuth multiplicative hash → map to [0, 31] → shift to [-16, 15].
    let hash = index.wrapping_mul(2_654_435_761);
    ((hash % 32) as i16) - 16
}

// ---------------------------------------------------------------------------
// EnergyVad
// ---------------------------------------------------------------------------

/// Lightweight energy-based Voice Activity Detector.
///
/// Speech is declared when the normalised RMS energy of a chunk exceeds a
/// configurable threshold.  A *hangover* timer keeps the detector active for a
/// short window after the last speech frame to avoid premature cut-off.
pub struct EnergyVad {
    threshold: f32,
    hangover: Duration,
    #[allow(dead_code)]
    sample_rate: u32,
    active: bool,
    last_speech: Option<Instant>,
}

impl EnergyVad {
    /// Create a new detector.
    ///
    /// * `threshold` — normalised RMS threshold (0.0–1.0).
    /// * `hangover_ms` — milliseconds to keep the detector active after the
    ///   last speech frame.
    /// * `sample_rate` — sample rate in Hz.
    pub fn new(threshold: f32, hangover_ms: u32, sample_rate: u32) -> Self {
        Self {
            threshold,
            hangover: Duration::from_millis(hangover_ms as u64),
            sample_rate,
            active: false,
            last_speech: None,
        }
    }
}

impl VadDetector for EnergyVad {
    fn process_samples(&mut self, samples: &[i16]) -> bool {
        if samples.is_empty() {
            return self.active;
        }

        let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / samples.len() as f64).sqrt();
        let normalised = rms / i16::MAX as f64;

        let now = Instant::now();

        if normalised > self.threshold as f64 {
            self.active = true;
            self.last_speech = Some(now);
        } else if self.active {
            // Stay active during hangover window.
            if let Some(last) = self.last_speech {
                if now.duration_since(last) > self.hangover {
                    self.active = false;
                    self.last_speech = None;
                }
            }
        }

        self.active
    }
}

// ---------------------------------------------------------------------------
// RelayActor
// ---------------------------------------------------------------------------

/// The main actor coordinating audio flow between a LiveKit room and an A2A
/// agent.
///
/// # Threading model
///
/// The actor runs entirely on the Tokio runtime.  It:
///
/// 1. Receives subscriber PCM via an unbounded channel.
/// 2. Runs VAD on each chunk and forwards audio to the A2A client.
/// 3. Receives agent response frames, validates the turn, and pushes them
///    into the jitter buffer.
/// 4. On a 10 ms tick, pops from the jitter buffer and writes to the
///    [`NativeAudioSource`] for WebRTC playout.
pub struct RelayActor<C, V> {
    #[allow(dead_code)]
    room: Arc<Room>,
    a2a_client: Arc<C>,
    #[allow(dead_code)]
    track: LocalAudioTrack,
    audio_source: NativeAudioSource,
    vad: V,
    turn_manager: Arc<TurnManager>,
    jitter_buffer: AudioJitterBuffer,
    sample_rate: u32,
    num_channels: u32,
}

impl<C, V> RelayActor<C, V>
where
    C: A2aClient,
    V: VadDetector,
{
    /// Build a new relay actor.
    pub fn new(
        room: Arc<Room>,
        a2a_client: Arc<C>,
        track: LocalAudioTrack,
        audio_source: NativeAudioSource,
        vad: V,
        sample_rate: u32,
        num_channels: u32,
    ) -> Self {
        Self {
            room,
            a2a_client,
            track,
            audio_source,
            vad,
            turn_manager: Arc::new(TurnManager::new()),
            jitter_buffer: AudioJitterBuffer::new(60, 200, sample_rate),
            sample_rate,
            num_channels,
        }
    }

    /// Return a handle to the shared [`TurnManager`].
    pub fn turn_manager(&self) -> Arc<TurnManager> {
        self.turn_manager.clone()
    }

    /// Run the actor loop until `shutdown_rx` signals `true`.
    ///
    /// `subscriber_audio_rx` carries PCM chunks captured from remote
    /// participants in the LiveKit room.
    pub async fn run(
        mut self,
        mut shutdown_rx: watch::Receiver<bool>,
        mut subscriber_audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    ) {
        let mut frame_rx = self.a2a_client.subscribe_frames();
        let mut playback_tick = tokio::time::interval(std::time::Duration::from_millis(10));

        info!("RelayActor: entering main loop (sr={}, ch={})", self.sample_rate, self.num_channels);

        loop {
            tokio::select! {
                biased;

                // 1. Shutdown
                result = shutdown_rx.changed() => {
                    if result.is_err() || *shutdown_rx.borrow() {
                        info!("RelayActor: shutdown signal received");
                        break;
                    }
                }

                // 2. Playback tick — pop jitter buffer → WebRTC
                _ = playback_tick.tick() => {
                    let samples_per_frame =
                        (self.sample_rate / 100) as usize * self.num_channels as usize;
                    let mut frame_data = vec![0i16; samples_per_frame];
                    self.jitter_buffer.pop(&mut frame_data);

                    let frame = AudioFrame {
                        data: frame_data.as_slice().into(),
                        sample_rate: self.sample_rate,
                        num_channels: self.num_channels,
                        samples_per_channel: (self.sample_rate / 100),
                    };
                    if let Err(e) = self.audio_source.capture_frame(&frame).await {
                        warn!("RelayActor: capture_frame error: {e}");
                    }
                }

                // 3. Subscriber audio → VAD + forward to A2A client
                Some(samples) = subscriber_audio_rx.recv() => {
                    let speech = self.vad.process_samples(&samples);

                    if speech {
                        let prev = self.turn_manager.current_turn();
                        let _new = self.turn_manager.next_turn();
                        self.audio_source.clear_buffer();
                        self.jitter_buffer.clear();
                        let _ = self.a2a_client.cancel_turn(prev).await;
                        let _ = self.a2a_client.request_floor().await;
                    }

                    let turn = self.turn_manager.current_turn();
                    if let Err(e) = self.a2a_client.send_audio(turn, &samples).await {
                        warn!("RelayActor: send_audio error: {e}");
                    }
                }

                // 4. A2A frames → turn filter → jitter buffer
                Some(frame) = frame_rx.recv() => {
                    if self.turn_manager.is_valid(frame.turn_id) {
                        self.jitter_buffer.push(&frame.samples);
                    } else {
                        debug!(
                            "RelayActor: dropping stale frame (frame turn={}, current={})",
                            frame.turn_id,
                            self.turn_manager.current_turn()
                        );
                    }
                }
            }
        }

        info!("RelayActor: exited main loop");
    }
}
