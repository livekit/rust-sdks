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

use std::path::Path;
#[cfg(use_tract)]
use std::sync::Once;

use ort::session::{builder::SessionBuilder, Session};

#[cfg(use_tract)]
static INIT_TRACT: Once = Once::new();

#[cfg(use_tract)]
pub(crate) fn ensure_tract_backend() {
    INIT_TRACT.call_once(|| {
        ort::set_api(ort_tract::api());
    });
}

pub(crate) mod embedding;
pub(crate) mod melspectrogram;
pub mod wakeword;

/// Graph optimizations requested when an ONNX session is created.
pub use ort::session::builder::GraphOptimizationLevel;
pub use wakeword::WakeWordModel;

#[derive(Debug, thiserror::Error)]
pub enum WakeWordError {
    #[error(transparent)]
    Ort(#[from] ort::Error),
    #[error(transparent)]
    Shape(#[from] ndarray::ShapeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("wake word model not found: {0}")]
    ModelNotFound(String),
    #[error("unsupported sample rate: {0} Hz")]
    UnsupportedSampleRate(u32),
    #[error(transparent)]
    Resample(#[from] resampler::ResampleError),
}

pub const SAMPLE_RATE: usize = 16000;
pub const MEL_BINS: usize = 32;
pub const EMBEDDING_WINDOW: usize = 76; // mel frames per embedding
pub const EMBEDDING_STRIDE: usize = 8; // mel frames between embeddings
pub const EMBEDDING_DIM: usize = 96;
pub const MIN_EMBEDDINGS: usize = 16; // classifier input length

pub(crate) fn to_resampler_rate(hz: u32) -> Result<resampler::SampleRate, WakeWordError> {
    use resampler::SampleRate;
    match hz {
        16000 => Ok(SampleRate::Hz16000),
        22050 => Ok(SampleRate::Hz22050),
        32000 => Ok(SampleRate::Hz32000),
        44100 => Ok(SampleRate::Hz44100),
        48000 => Ok(SampleRate::Hz48000),
        88200 => Ok(SampleRate::Hz88200),
        96000 => Ok(SampleRate::Hz96000),
        176400 => Ok(SampleRate::Hz176400),
        192000 => Ok(SampleRate::Hz192000),
        384000 => Ok(SampleRate::Hz384000),
        _ => Err(WakeWordError::UnsupportedSampleRate(hz)),
    }
}

/// How every ONNX session a [`WakeWordModel`] creates is configured: the two
/// bundled feature extraction models and each wake word classifier. Mirrors the
/// `sess_options` parameter the Python SDK takes.
///
/// [`Default`] is what [`WakeWordModel::new`] uses. Pass a value of your own to
/// [`WakeWordModel::with_session_options`] to trade latency for CPU, which is
/// useful when detection runs as a background process on a small machine:
///
/// ```no_run
/// # use livekit_wakeword::{SessionOptions, WakeWordModel};
/// # fn main() -> Result<(), livekit_wakeword::WakeWordError> {
/// let options = SessionOptions {
///     intra_threads: Some(1),
///     inter_threads: Some(1),
///     parallel_execution: Some(false),
///     intra_op_spinning: Some(false),
///     inter_op_spinning: Some(false),
///     ..Default::default()
/// };
/// let model = WakeWordModel::with_session_options(&["hey_livekit.onnx"], 16000, options)?;
/// # Ok(())
/// # }
/// ```
///
/// `optimization_level` is the only option the `ort-tract` backend used on every
/// target except aarch64 Windows implements; the rest are skipped there rather than
/// failing session creation, and since tract runs single-threaded without
/// spin-waiting, they already describe what it does.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    /// Graph optimizations to apply when the session is created. Matters more than
    /// it looks: `ort-tract` runs tract's `into_optimized()` only when some level is
    /// requested, and wake word inference measured several times slower without it.
    pub optimization_level: GraphOptimizationLevel,
    /// Threads used to run a single operator. `Some(1)` keeps each session on the
    /// calling thread.
    pub intra_threads: Option<usize>,
    /// Threads used to run operators in parallel, when `parallel_execution` is on.
    pub inter_threads: Option<usize>,
    /// Whether independent operators may run in parallel. `Some(false)` is ONNX
    /// Runtime's sequential execution mode.
    pub parallel_execution: Option<bool>,
    /// Whether intra-op threads may spin before blocking. `Some(false)` trades a
    /// little latency for markedly less CPU while idle.
    pub intra_op_spinning: Option<bool>,
    /// Whether inter-op threads may spin before blocking.
    pub inter_op_spinning: Option<bool>,
    /// Session config entries applied verbatim, for anything the fields above do
    /// not cover.
    pub config_entries: Vec<(String, String)>,
}

impl Default for SessionOptions {
    /// Requests [`GraphOptimizationLevel::Level3`], ONNX Runtime's own default, and
    /// leaves every other option to the backend.
    fn default() -> Self {
        Self {
            optimization_level: GraphOptimizationLevel::Level3,
            intra_threads: None,
            inter_threads: None,
            parallel_execution: None,
            intra_op_spinning: None,
            inter_op_spinning: None,
            config_entries: Vec::new(),
        }
    }
}

impl SessionOptions {
    // The optimization level is the only session option `ort-tract` implements;
    // every other one resolves to `ort-sys`' stub API and reports `NotImplemented`.
    // Since tract runs single-threaded without spin-waiting, skipping those is
    // closer to what the caller asked for than refusing to build the session.
    #[cfg(use_tract)]
    fn apply(&self, builder: SessionBuilder) -> Result<SessionBuilder, WakeWordError> {
        Ok(builder.with_optimization_level(self.optimization_level)?)
    }

    #[cfg(not(use_tract))]
    fn apply(&self, builder: SessionBuilder) -> Result<SessionBuilder, WakeWordError> {
        let mut builder = builder.with_optimization_level(self.optimization_level)?;
        if let Some(threads) = self.intra_threads {
            builder = builder.with_intra_threads(threads)?;
        }
        if let Some(threads) = self.inter_threads {
            builder = builder.with_inter_threads(threads)?;
        }
        if let Some(parallel) = self.parallel_execution {
            builder = builder.with_parallel_execution(parallel)?;
        }
        if let Some(enable) = self.intra_op_spinning {
            builder = builder.with_intra_op_spinning(enable)?;
        }
        if let Some(enable) = self.inter_op_spinning {
            builder = builder.with_inter_op_spinning(enable)?;
        }
        for (key, value) in &self.config_entries {
            builder = builder.with_config_entry(key, value)?;
        }
        Ok(builder)
    }
}

pub(crate) fn build_session_from_memory(
    bytes: &[u8],
    options: &SessionOptions,
) -> Result<Session, WakeWordError> {
    #[cfg(use_tract)]
    ensure_tract_backend();
    Ok(options.apply(Session::builder()?)?.commit_from_memory(bytes)?)
}

pub(crate) fn build_session_from_file(
    path: impl AsRef<Path>,
    options: &SessionOptions,
) -> Result<Session, WakeWordError> {
    build_session_from_memory(&std::fs::read(path)?, options)
}
