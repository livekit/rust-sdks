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

use ndarray::{Array1, Array2, Axis};
use ort::session::Session;
use ort::value::Tensor;

use crate::build_session_from_memory;

const MODEL_BYTES: &[u8] = include_bytes!("../onnx/melspectrogram.onnx");

// Extracts mel-scaled spectrogram features from raw audio using a pre-trained ONNX model.
//
// Model input:  f32 tensor of shape (1, num_samples) — mono PCM audio normalized to [-1.0, 1.0]
// Model output: f32 tensor of shape (1, 1, time_frames, mel_bins) — e.g. (1, 1, 97, 32) for 16000 samples
//
// The detect() method accepts i16 PCM samples, handles normalization internally,
// and returns the mel features as an Array2<f32> of shape (time_frames, mel_bins).
pub struct MelspectrogramModel {
    session: Session,
}

impl MelspectrogramModel {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { session: build_session_from_memory(MODEL_BYTES)? })
    }

    // Run the melspectrogram model on raw audio and return mel features.
    // Input: slice of i16 PCM samples.
    // Output: Array2<f32> of shape (time_frames, mel_bins) e.g. (97, 32).
    pub fn detect(
        &mut self,
        samples: &[i16],
    ) -> Result<Array2<f32>, Box<dyn std::error::Error>> {
        let audio_f32: Array1<f32> = samples.iter().map(|&x| (x as f32) / 32768.0).collect();

        let audio_2d = audio_f32.insert_axis(Axis(0));
        let audio_tensor = Tensor::from_array(audio_2d)?;

        let features = self.session.run(ort::inputs![audio_tensor])?;

        let raw = features["output"].try_extract_array::<f32>()?;
        let rows = raw.shape()[2];
        let cols = raw.shape()[3];
        let output = raw.into_owned().into_shape_with_order((rows, cols))?;

        Ok(output)
    }
}
