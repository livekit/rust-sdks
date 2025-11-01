// Copyright 2025 LiveKit, Inc.
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

use livekit_audio::resampler::{
    Resampler as ResamplerInner, ResamplerError, ResamplerQuality, ResamplerSettings,
};
use std::sync::Mutex;

#[uniffi::remote(Error)]
pub enum ResamplerError {
    Initialization(String),
    OperationFailed(String),
}

#[uniffi::remote(Enum)]
pub enum ResamplerQuality {
    Quick,
    Low,
    Medium,
    High,
    VeryHigh,
}

#[uniffi::remote(Record)]
pub struct ResamplerSettings {
    pub input_rate: f64,
    pub output_rate: f64,
    pub num_channels: u32,
    pub quality: ResamplerQuality,
}

#[derive(uniffi::Object)]
pub struct Resampler {
    inner: Mutex<ResamplerInner>,
}

#[uniffi::export]
impl Resampler {
    /// Creates a new audio resampler with the given settings.
    #[uniffi::constructor]
    pub fn new(settings: ResamplerSettings) -> Result<Self, ResamplerError> {
        Ok(Self { inner: ResamplerInner::new(settings)?.into() })
    }

    /// Push audio data into the resampler and retrieve any available resampled data.
    ///
    /// This method accepts audio data, resamples it according to the configured input
    /// and output rates, and returns any resampled data that is available after processing the input.
    ///
    pub fn push(&self, input: &[i16]) -> Result<Vec<i16>, ResamplerError> {
        let mut inner = self.inner.lock().unwrap();
        Ok(inner.push(input)?.to_vec())
    }

    /// Flush any remaining audio data through the resampler and retrieve the resampled data.
    ///
    /// This method should be called when no more input data will be provided to ensure that all
    /// internal buffers are processed and all resampled data is output.
    ///
    pub fn flush(&self) -> Result<Vec<i16>, ResamplerError> {
        let mut inner = self.inner.lock().unwrap();
        Ok(inner.flush()?.to_vec())
    }
}
