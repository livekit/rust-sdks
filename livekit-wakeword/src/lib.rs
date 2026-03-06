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

use ort::session::Session;

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

pub(crate) fn build_session_from_memory(bytes: &[u8]) -> Result<Session, WakeWordError> {
    #[cfg(use_tract)]
    ensure_tract_backend();
    Ok(Session::builder()?.commit_from_memory(bytes)?)
}

pub(crate) fn build_session_from_file(path: impl AsRef<Path>) -> Result<Session, WakeWordError> {
    #[cfg(use_tract)]
    ensure_tract_backend();
    let bytes = std::fs::read(path)?;
    Ok(Session::builder()?.commit_from_memory(&bytes)?)
}
