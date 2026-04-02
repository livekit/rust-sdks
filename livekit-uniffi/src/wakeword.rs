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

use livekit_wakeword::{WakeWordError, WakeWordModel};
use std::collections::HashMap;
use std::sync::Mutex;

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum WakeWordError {
    Ort,
    Shape,
    Io,
    ModelNotFound,
    UnsupportedSampleRate,
    Resample,
}

/// Wake word detector backed by ONNX classifier models.
///
/// Wraps [`livekit_wakeword::WakeWordModel`] for use across FFI boundaries.
/// Uses interior mutability since the underlying model requires `&mut self`.
#[derive(uniffi::Object)]
pub struct WakeWordDetector {
    inner: Mutex<WakeWordModel>,
}

#[uniffi::export]
impl WakeWordDetector {
    /// Create a new wake word detector.
    ///
    /// `model_paths` are filesystem paths to ONNX classifier models.
    /// `sample_rate` is the sample rate of audio that will be passed to
    /// [`predict`](Self::predict). Supported rates: 16000 (recommended),
    /// 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000 Hz.
    #[uniffi::constructor]
    pub fn new(model_paths: Vec<String>, sample_rate: u32) -> Result<Self, WakeWordError> {
        let model = WakeWordModel::new(&model_paths, sample_rate)?;
        Ok(Self { inner: Mutex::new(model) })
    }

    /// Load an additional wake word classifier ONNX model from disk.
    ///
    /// If `model_name` is `None`, the file stem is used as the classifier name.
    pub fn load_model(
        &self,
        model_path: String,
        model_name: Option<String>,
    ) -> Result<(), WakeWordError> {
        let mut inner = self.inner.lock().unwrap();
        inner.load_model(&model_path, model_name.as_deref())?;
        Ok(())
    }

    /// Get wake word predictions for an audio chunk.
    ///
    /// Pass ~2 seconds of i16 PCM audio at the sample rate configured in
    /// [`new`](Self::new). Returns a map of classifier name to confidence score.
    pub fn predict(&self, audio_chunk: Vec<i16>) -> Result<HashMap<String, f32>, WakeWordError> {
        let mut inner = self.inner.lock().unwrap();
        Ok(inner.predict(&audio_chunk)?)
    }
}
