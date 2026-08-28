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

use ort::session::{
    builder::{GraphOptimizationLevel, SessionBuilder},
    Session,
};

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

/// Graph optimization level applied to the ONNX graph when a session is created.
///
/// The default, [`Level3`](Self::Level3), matches ONNX Runtime's own default. It
/// also matters more than it looks: the `ort-tract` backend used on every target
/// except aarch64 Windows runs tract's `into_optimized()` only when a session asks
/// for some level of optimization, and wake word inference measured several times
/// slower on the unoptimized graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// Run the graph as it was loaded.
    Disable,
    /// Semantics-preserving rewrites that remove redundant nodes and computation.
    Level1,
    /// Level 1, plus fusions that depend on the execution provider.
    Level2,
    /// Level 2, plus layout optimizations. ONNX Runtime's default.
    #[default]
    Level3,
}

impl From<OptimizationLevel> for GraphOptimizationLevel {
    fn from(level: OptimizationLevel) -> Self {
        match level {
            OptimizationLevel::Disable => GraphOptimizationLevel::Disable,
            OptimizationLevel::Level1 => GraphOptimizationLevel::Level1,
            OptimizationLevel::Level2 => GraphOptimizationLevel::Level2,
            OptimizationLevel::Level3 => GraphOptimizationLevel::Level3,
        }
    }
}

/// Tuning applied to every ONNX session a [`WakeWordModel`] creates: the two
/// bundled feature extraction models and each wake word classifier.
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
/// Every field except `optimization_level` is best-effort: a backend that does not
/// implement an option has it skipped rather than failing session creation. In
/// particular the `ort-tract` backend implements only the optimization level, and
/// runs single-threaded without spin-waiting, so the thread and spinning fields
/// above describe what it already does.
#[derive(Clone, Debug, Default)]
pub struct SessionOptions {
    /// Graph optimizations to apply when the session is created.
    pub optimization_level: OptimizationLevel,
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

impl SessionOptions {
    fn apply(&self, builder: SessionBuilder) -> Result<SessionBuilder, WakeWordError> {
        // Unlike everything below, the optimization level is implemented by every
        // backend, so a failure here is a real error rather than an unsupported knob.
        let mut builder = builder.with_optimization_level(self.optimization_level.into())?;

        if let Some(threads) = self.intra_threads {
            builder = best_effort(builder, |b| b.with_intra_threads(threads))?;
        }
        if let Some(threads) = self.inter_threads {
            builder = best_effort(builder, |b| b.with_inter_threads(threads))?;
        }
        if let Some(parallel) = self.parallel_execution {
            builder = best_effort(builder, |b| b.with_parallel_execution(parallel))?;
        }
        if let Some(enable) = self.intra_op_spinning {
            builder = best_effort(builder, |b| b.with_intra_op_spinning(enable))?;
        }
        if let Some(enable) = self.inter_op_spinning {
            builder = best_effort(builder, |b| b.with_inter_op_spinning(enable))?;
        }
        for (key, value) in &self.config_entries {
            builder = best_effort(builder, |b| b.with_config_entry(key, value))?;
        }

        Ok(builder)
    }
}

// Applies one session option, keeping the builder unchanged if the active backend
// has no implementation for it. `ort-tract` implements only the graph optimization
// level; every other session option resolves to `ort-sys`' stub API and reports
// `NotImplemented`. Since tract already runs single-threaded and never spin-waits,
// skipping such an option is closer to what the caller asked for than refusing to
// build the session at all.
fn best_effort(
    builder: SessionBuilder,
    f: impl FnOnce(SessionBuilder) -> ort::Result<SessionBuilder>,
) -> Result<SessionBuilder, WakeWordError> {
    let unchanged = builder.clone();
    match f(builder) {
        Ok(builder) => Ok(builder),
        Err(err) if err.code() == ort::ErrorCode::NotImplemented => Ok(unchanged),
        Err(err) => Err(err.into()),
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
    #[cfg(use_tract)]
    ensure_tract_backend();
    let bytes = std::fs::read(path)?;
    Ok(options.apply(Session::builder()?)?.commit_from_memory(&bytes)?)
}
