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

//! Types shared by the capture pumps.
//!
//! The pumps live with their frame kinds:
//! [`PixelVideoPump`](crate::pixel::PixelVideoPump) pumps pixel frames, and
//! [`EncodedVideoPump`](crate::encoded::EncodedVideoPump) pumps encoded
//! access units. Both spawn into the same [`RunningPump`].

use crate::error::SourceError;
use std::{
    any::Any,
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};
use thiserror::Error;

/// Error returned by a pump run.
#[derive(Debug, Error)]
pub enum PumpError {
    /// The capture source failed or violated its contract.
    #[error("capture source failed: {0}")]
    Source(#[from] SourceError),
    /// The RTC source rejected a frame.
    #[error("capture source rejected the frame")]
    CaptureFailed,
    /// The pump thread panicked.
    #[error("pump panicked: {0}")]
    Panicked(String),
}

/// Why a pump run ended successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpExit {
    /// The stop handle fired.
    Stopped,
    /// The source reached the end of its stream.
    EndOfStream,
}

/// Statistics returned when a pump run ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PumpStats {
    /// Number of frames or access units captured.
    pub frames_captured: u64,
    /// Why the run ended.
    pub exit: PumpExit,
}

/// Cancellation handle for a pump.
///
/// The handle is cheap to clone. Call [`PumpStop::stop`] from any thread to
/// make the pump return after the frame in flight.
#[derive(Debug, Clone, Default)]
pub struct PumpStop(Arc<AtomicBool>);

impl PumpStop {
    /// Creates a new handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals the pump to stop.
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns whether [`PumpStop::stop`] was called.
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Spawns a pump run on a dedicated thread with panic capture and a
/// completion signal.
pub(crate) fn spawn_pump(
    stop: PumpStop,
    run: impl FnOnce() -> Result<PumpStats, PumpError> + Send + 'static,
) -> io::Result<RunningPump> {
    let (finished_tx, finished_rx) = tokio::sync::watch::channel(false);

    let thread = thread::Builder::new().name("lk-video-pump".to_owned()).spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(run))
            .unwrap_or_else(|panic| Err(PumpError::Panicked(panic_message(&*panic))));
        let _ = finished_tx.send(true);
        result
    })?;

    Ok(RunningPump { stop, thread, finished: finished_rx })
}

/// Renders a panic payload for [`PumpError::Panicked`].
fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "opaque panic payload".to_owned()
    }
}

/// A pump of either kind that runs on a dedicated thread.
///
/// A stop takes effect between frames: a source that is blocked on its next
/// frame completes that wait before the pump observes the signal.
#[derive(Debug)]
pub struct RunningPump {
    stop: PumpStop,
    thread: thread::JoinHandle<Result<PumpStats, PumpError>>,
    // Flipped to true by the pump thread just before it exits.
    finished: tokio::sync::watch::Receiver<bool>,
}

impl RunningPump {
    /// Returns a cancellation handle for the pump.
    pub fn stop_handle(&self) -> PumpStop {
        self.stop.clone()
    }

    /// Signals the pump to stop after the frame in flight.
    pub fn stop(&self) {
        self.stop.stop();
    }

    /// Returns whether the pump thread exited.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Waits for the pump thread to exit.
    ///
    /// Panics on the pump thread are reported as [`PumpError::Panicked`].
    pub fn join(self) -> Result<PumpStats, PumpError> {
        self.thread.join().unwrap_or_else(|panic| Err(PumpError::Panicked(panic_message(&*panic))))
    }

    /// Signals the pump to stop and waits for its thread to exit.
    pub fn stop_and_join(self) -> Result<PumpStats, PumpError> {
        self.stop();
        self.join()
    }

    /// Waits for the pump thread to exit without blocking the async runtime.
    ///
    /// This works under any async runtime, not only tokio, and is safe to
    /// hold across long waits — for example in a `select!` that supervises
    /// every running pump. Panics on the pump thread are reported as
    /// [`PumpError::Panicked`].
    pub async fn join_async(mut self) -> Result<PumpStats, PumpError> {
        // An error means the sender dropped, which also implies the pump
        // thread is done; either way the join below returns promptly.
        let _ = self.finished.wait_for(|finished| *finished).await;
        self.join()
    }

    /// Signals the pump to stop and waits for its thread to exit without
    /// blocking the async runtime.
    pub async fn stop_and_join_async(self) -> Result<PumpStats, PumpError> {
        self.stop();
        self.join_async().await
    }
}
