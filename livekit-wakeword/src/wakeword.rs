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

use std::collections::HashMap;
use std::path::Path;

use ndarray::Axis;
use ort::session::Session;
use ort::value::Tensor;
use resampler::{ResamplerFft, SampleRate};

use crate::embedding::EmbeddingModel;
use crate::melspectrogram::MelspectrogramModel;
use crate::{
    build_session_from_file, to_resampler_rate, WakeWordError, EMBEDDING_STRIDE, EMBEDDING_WINDOW,
    MIN_EMBEDDINGS,
};

struct Resampler {
    fft: ResamplerFft,
    input_buf: Vec<f32>,
    output_buf: Vec<f32>,
    input_rate: u32,
}

/// Wake word detection model with optional input resampling.
///
/// The mel spectrogram and speech embedding models are bundled at compile time.
/// Wake word classifier models are loaded dynamically from disk at runtime.
///
/// Pass ~2 seconds of i16 PCM audio at the configured sample rate to
/// [`predict`](Self::predict) and receive per-classifier confidence scores.
pub struct WakeWordModel {
    mel_model: MelspectrogramModel,
    emb_model: EmbeddingModel,
    classifiers: HashMap<String, Session>,
    resampler: Option<Resampler>,
}

impl WakeWordModel {
    /// Create a new wake word model.
    ///
    /// The recommended sample rate is 16 kHz. Other supported rates
    /// (22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000 Hz)
    /// are resampled internally to 16 kHz.
    pub fn new(models: &[impl AsRef<Path>], sample_rate: u32) -> Result<Self, WakeWordError> {
        let resampler = if sample_rate != 16000 {
            let input_rate = to_resampler_rate(sample_rate)?;
            let fft = ResamplerFft::new(1, input_rate, SampleRate::Hz16000);
            let input_buf = vec![0.0f32; fft.chunk_size_input()];
            let output_buf = vec![0.0f32; fft.chunk_size_output()];
            Some(Resampler { fft, input_buf, output_buf, input_rate: sample_rate })
        } else {
            None
        };

        let mut wakeword = Self {
            mel_model: MelspectrogramModel::new()?,
            emb_model: EmbeddingModel::new()?,
            classifiers: HashMap::new(),
            resampler,
        };

        for path in models {
            wakeword.load_model(path, None)?;
        }

        Ok(wakeword)
    }

    /// Load a wake word classifier ONNX model from disk.
    ///
    /// If `model_name` is `None`, the file stem is used as the classifier name.
    pub fn load_model(
        &mut self,
        model_path: impl AsRef<Path>,
        model_name: Option<&str>,
    ) -> Result<(), WakeWordError> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(WakeWordError::ModelNotFound(path.display().to_string()));
        }

        let name = match model_name {
            Some(n) => n.to_string(),
            None => path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
        };

        let session = build_session_from_file(path)?;
        self.classifiers.insert(name, session);
        Ok(())
    }

    fn resample_to_16k(&mut self, samples: &[i16]) -> Result<Vec<f32>, WakeWordError> {
        let rs = self.resampler.as_mut().unwrap();
        let chunk_in = rs.fft.chunk_size_input();
        let chunk_out = rs.fft.chunk_size_output();

        // Expected output length based on ratio
        let expected_len =
            (samples.len() as f64 * 16000.0 / rs.input_rate as f64).round() as usize;

        let mut output = Vec::with_capacity(expected_len);

        let mut pos = 0;
        while pos < samples.len() {
            let remaining = samples.len() - pos;
            let n = remaining.min(chunk_in);

            // Fill input buffer, zero-pad if last chunk is short
            for (i, v) in rs.input_buf.iter_mut().enumerate() {
                *v = if i < n { samples[pos + i] as f32 / 32768.0 } else { 0.0 };
            }

            rs.fft.resample(&rs.input_buf.clone(), &mut rs.output_buf)?;

            let take = if remaining < chunk_in {
                // Last (partial) chunk: scale output proportionally
                (n as f64 * chunk_out as f64 / chunk_in as f64).round() as usize
            } else {
                chunk_out
            };

            output.extend_from_slice(&rs.output_buf[..take]);

            pos += chunk_in;
        }

        output.truncate(expected_len);
        Ok(output)
    }

    /// Get wake word predictions for an audio chunk.
    ///
    /// Pass ~2 seconds of i16 PCM audio at the sample rate configured in
    /// [`new`](Self::new). Shorter chunks that produce fewer than
    /// [`MIN_EMBEDDINGS`] embeddings return zero scores.
    pub fn predict(
        &mut self,
        audio_chunk: &[i16],
    ) -> Result<HashMap<String, f32>, WakeWordError> {
        if self.classifiers.is_empty() {
            return Ok(HashMap::new());
        }

        // Resample if needed, then normalize to f32
        let samples_f32 = if self.resampler.is_some() {
            self.resample_to_16k(audio_chunk)?
        } else {
            audio_chunk.iter().map(|&x| x as f32 / 32768.0).collect()
        };

        // Mel spectrogram over the full chunk
        let mel = self.mel_model.detect(&samples_f32)?;
        let num_frames = mel.shape()[0];

        if num_frames < EMBEDDING_WINDOW {
            return Ok(self.zero_scores());
        }

        // Extract embeddings: 76-frame windows, stride 8
        let mut embeddings = Vec::new();
        let mut start = 0;
        while start + EMBEDDING_WINDOW <= num_frames {
            let window = mel.slice(ndarray::s![start..start + EMBEDDING_WINDOW, ..]);
            let window_slice = window.as_standard_layout();
            let emb = self.emb_model.detect(window_slice.as_slice().unwrap())?;
            embeddings.push(emb);
            start += EMBEDDING_STRIDE;
        }

        if embeddings.len() < MIN_EMBEDDINGS {
            return Ok(self.zero_scores());
        }

        // Use last MIN_EMBEDDINGS embeddings -> shape (1, 16, 96)
        let last = &embeddings[embeddings.len() - MIN_EMBEDDINGS..];
        let views: Vec<_> = last.iter().map(|e| e.view()).collect();
        let emb_sequence = ndarray::stack(Axis(0), &views)?;
        let emb_input = emb_sequence.insert_axis(Axis(0));

        // Run each classifier
        let mut predictions = HashMap::new();
        for (name, session) in &mut self.classifiers {
            let tensor = Tensor::from_array(emb_input.clone())?;
            let outputs = session.run(ort::inputs!["embeddings" => tensor])?;
            let raw = outputs["score"].try_extract_array::<f32>()?;
            let score = raw.iter().copied().next().unwrap_or(0.0);
            predictions.insert(name.clone(), score);
        }

        Ok(predictions)
    }

    fn zero_scores(&self) -> HashMap<String, f32> {
        self.classifiers.keys().map(|k| (k.clone(), 0.0)).collect()
    }
}
