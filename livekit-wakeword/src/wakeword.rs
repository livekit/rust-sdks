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

use resampler::{Attenuation, Latency, ResamplerFir, SampleRate};

use crate::stream::StreamingClassifier;
use crate::{build_session_from_file, to_resampler_rate, SessionOptions, WakeWordError};

struct Resampler {
    fir: ResamplerFir,
    output_buf: Vec<f32>,
    input_rate: u32,
}

impl Resampler {
    fn new(sample_rate: u32) -> Result<Self, WakeWordError> {
        let input_rate = to_resampler_rate(sample_rate)?;
        // FIR resampler: 64-sample latency (~1.3ms at 48kHz) with 90dB
        // stopband attenuation to match the quality of training data.
        let fir = ResamplerFir::new(
            1,
            input_rate,
            SampleRate::Hz16000,
            Latency::Sample64,
            Attenuation::Db90,
        );
        let output_buf = vec![0.0f32; fir.buffer_size_output()];
        Ok(Self { fir, output_buf, input_rate: sample_rate })
    }

    fn resample(&mut self, input: &[f32]) -> Result<Vec<f32>, WakeWordError> {
        let mut output = Vec::with_capacity(
            (input.len() as f64 * 16000.0 / self.input_rate as f64).ceil() as usize,
        );
        let mut pos = 0;
        while pos < input.len() {
            let (consumed, produced) = self.fir.resample(&input[pos..], &mut self.output_buf)?;
            output.extend_from_slice(&self.output_buf[..produced]);
            pos += consumed;
            if consumed == 0 && produced == 0 {
                break;
            }
        }
        Ok(output)
    }
}

/// Streaming wake word detector: feed PCM as it arrives, get per-model scores back.
///
/// Each model is one `.onnx` file exported by `livekit-wakeword train`, keyed by its
/// file stem. Every model keeps its own state across [`predict`](Self::predict)
/// calls and steps once per [`FRAME_SAMPLES`](crate::FRAME_SAMPLES) of 16 kHz audio
/// (80 ms), so audio can be passed in chunks of any size.
pub struct WakeWordModel {
    models: HashMap<String, StreamingClassifier>,
    resampler: Option<Resampler>,
    session_options: SessionOptions,
}

impl WakeWordModel {
    /// Create a wake word model from streaming model files.
    ///
    /// `sample_rate` is the rate of the PCM passed to [`predict`](Self::predict).
    /// The recommended rate is 16 kHz; other supported rates (22050, 32000, 44100,
    /// 48000, 88200, 96000, 176400, 192000, 384000 Hz) are resampled internally.
    ///
    /// ONNX sessions are created with [`SessionOptions::default`]; use
    /// [`with_session_options`](Self::with_session_options) to tune them.
    pub fn new(models: &[impl AsRef<Path>], sample_rate: u32) -> Result<Self, WakeWordError> {
        Self::with_session_options(models, sample_rate, SessionOptions::default())
    }

    /// Create a wake word model, tuning the ONNX sessions it creates.
    ///
    /// `session_options` applies to every model in `models` and to any model a later
    /// [`load_model`](Self::load_model) call adds.
    pub fn with_session_options(
        models: &[impl AsRef<Path>],
        sample_rate: u32,
        session_options: SessionOptions,
    ) -> Result<Self, WakeWordError> {
        let resampler = if sample_rate != crate::SAMPLE_RATE as u32 {
            Some(Resampler::new(sample_rate)?)
        } else {
            None
        };

        let mut wakeword = Self { models: HashMap::new(), resampler, session_options };
        for path in models {
            wakeword.load_model(path, None)?;
        }
        Ok(wakeword)
    }

    /// Load a streaming wake word model from disk, replacing any model of the same name.
    ///
    /// If `model_name` is `None`, the file stem is used as the name. Fails with
    /// [`WakeWordError::NotStreamingModel`] for anything other than a streaming model,
    /// including v1 classifier models.
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

        let session = build_session_from_file(path, &self.session_options)?;
        let model = StreamingClassifier::new(session, &path.display().to_string())?;
        self.models.insert(name, model);
        Ok(())
    }

    /// Remove a model, returning `true` if it was loaded.
    pub fn unload_model(&mut self, model_name: &str) -> bool {
        self.models.remove(model_name).is_some()
    }

    /// Names of the loaded models.
    pub fn model_names(&self) -> impl Iterator<Item = &str> {
        self.models.keys().map(String::as_str)
    }

    /// Feed the next chunk of i16 PCM at the configured sample rate, of any length.
    ///
    /// Returns each model's highest score among the 80 ms steps this call
    /// completed, or `0.0` if it completed none. Samples that do not fill a step are
    /// kept for the next call.
    pub fn predict(&mut self, audio_chunk: &[i16]) -> Result<HashMap<String, f32>, WakeWordError> {
        if self.models.is_empty() {
            return Ok(HashMap::new());
        }

        let samples: Vec<f32> = audio_chunk.iter().map(|&x| x as f32 / 32768.0).collect();
        let samples = match self.resampler.as_mut() {
            Some(resampler) => resampler.resample(&samples)?,
            None => samples,
        };

        let mut scores = HashMap::with_capacity(self.models.len());
        for (name, model) in &mut self.models {
            scores.insert(name.clone(), model.feed(&samples)?.unwrap_or(0.0));
        }
        Ok(scores)
    }

    /// Score a whole clip from a fresh state: each model's highest score in it.
    ///
    /// Equivalent to [`reset`](Self::reset) followed by [`predict`](Self::predict).
    pub fn predict_clip(&mut self, clip: &[i16]) -> Result<HashMap<String, f32>, WakeWordError> {
        self.reset()?;
        self.predict(clip)
    }

    /// Forget all audio heard so far, e.g. after a detection.
    pub fn reset(&mut self) -> Result<(), WakeWordError> {
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.fir.reset();
        }
        for model in self.models.values_mut() {
            model.reset()?;
        }
        Ok(())
    }
}
