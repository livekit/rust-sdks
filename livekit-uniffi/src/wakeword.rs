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

use std::path::PathBuf;

use livekit_wakeword::{WakeWordError, WakeWordModel};
use parking_lot::Mutex;

/// An error that can occur during wake word detection.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum WakeWordFfiError {
    #[error("{0}")]
    Model(String),
}

impl From<WakeWordError> for WakeWordFfiError {
    fn from(e: WakeWordError) -> Self {
        Self::Model(e.to_string())
    }
}

/// A per-classifier wake word score in the range [0.0, 1.0].
#[derive(uniffi::Record)]
pub struct WakeWordScore {
    pub name: String,
    pub score: f32,
}

/// ONNX-based wake word detector.
///
/// Wraps [`livekit_wakeword::WakeWordModel`] behind a thread-safe interface
/// suitable for UniFFI. Feed ~2 seconds of i16 PCM audio at the configured
/// sample rate into [`predict`](Self::predict) and receive per-classifier
/// confidence scores.
#[derive(uniffi::Object)]
pub struct WakeWordDetector {
    inner: Mutex<WakeWordModel>,
}

#[uniffi::export]
impl WakeWordDetector {
    /// Create a new detector.
    ///
    /// `classifier_paths` is a list of filesystem paths to classifier `.onnx`
    /// files (e.g. `hey_livekit.onnx`). `sample_rate` is the sample rate of
    /// the audio that will be passed to [`predict`](Self::predict); anything
    /// other than 16 kHz is resampled internally.
    #[uniffi::constructor]
    pub fn new(
        classifier_paths: Vec<String>,
        sample_rate: u32,
    ) -> Result<Self, WakeWordFfiError> {
        let paths: Vec<PathBuf> = classifier_paths.into_iter().map(PathBuf::from).collect();
        let model = WakeWordModel::new(&paths, sample_rate)?;
        Ok(Self { inner: Mutex::new(model) })
    }

    /// Load an additional classifier `.onnx` model at runtime.
    ///
    /// If `name` is omitted, the file stem is used as the classifier name.
    pub fn load_model(
        &self,
        path: String,
        name: Option<String>,
    ) -> Result<(), WakeWordFfiError> {
        self.inner.lock().load_model(&path, name.as_deref())?;
        Ok(())
    }

    /// Run inference on a chunk of i16 PCM audio.
    ///
    /// Pass ~2 seconds of mono i16 PCM at the sample rate configured in
    /// [`new`](Self::new). Shorter chunks return zero scores.
    pub fn predict(&self, pcm_i16: Vec<i16>) -> Result<Vec<WakeWordScore>, WakeWordFfiError> {
        let map = self.inner.lock().predict(&pcm_i16)?;
        Ok(map.into_iter().map(|(name, score)| WakeWordScore { name, score }).collect())
    }
}
