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

use std::sync::{Arc, OnceLock};

use livekit::webrtc::native::audio_resampler as rtc;
use parking_lot::Mutex;

use super::audio_stream::AudioFrameBuffer;
use crate::FfiHandleId;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AudioResamplerError {
    #[error("{0}")]
    MalformedBuffer(String),
}

/// libwebrtc's resampler, which [`crate::server::resampler::SoxResampler`] supersedes.
///
/// It is migrated because it is the second producer of an [`AudioFrameBuffer`], which is
/// the edge the audio layer was pricing.
///
/// TODO: delete this with `RemixAndResample`, which is already marked for deprecation —
/// not before it, since the request has no other implementation.
#[derive(uniffi::Object)]
pub struct AudioResampler {
    inner: Mutex<rtc::AudioResampler>,
    /// Reached by [`crate::migration::audio_resampler`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}

#[uniffi::export]
impl AudioResampler {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(rtc::AudioResampler::default()),
            handle_id: OnceLock::new(),
        })
    }

    /// Resamples `buffer` to `sample_rate`, mixed down or up to `num_channels`.
    ///
    /// A buffer's `samples_per_channel` and `num_channels` describe its `data`, and the
    /// native resampler asserts that they do. Both come from the foreign side, so they are
    /// checked here: an assert there would unwind across the boundary rather than return.
    pub fn remix_and_resample(
        &self,
        buffer: AudioFrameBuffer,
        num_channels: u32,
        sample_rate: u32,
    ) -> Result<AudioFrameBuffer, AudioResamplerError> {
        if buffer.num_channels == 0 || num_channels == 0 {
            return Err(AudioResamplerError::MalformedBuffer(
                "a channel count of zero carries no samples".into(),
            ));
        }
        let declared = buffer.samples_per_channel as usize * buffer.num_channels as usize;
        if buffer.data.len() < declared {
            return Err(AudioResamplerError::MalformedBuffer(format!(
                "{} channels of {} samples need {declared} samples, got {}",
                buffer.num_channels,
                buffer.samples_per_channel,
                buffer.data.len()
            )));
        }

        let mut inner = self.inner.lock();
        let data = inner
            .remix_and_resample(
                &buffer.data,
                buffer.samples_per_channel,
                buffer.num_channels,
                buffer.sample_rate,
                num_channels,
                sample_rate,
            )
            .to_vec();

        Ok(AudioFrameBuffer {
            sample_rate,
            num_channels,
            samples_per_channel: data.len() as u32 / num_channels,
            data,
        })
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    /// 10ms of 48kHz mono in, 10ms of 24kHz mono out, at the level it went in at.
    #[test]
    fn a_frame_comes_back_at_the_rate_asked_for() {
        let resampler = AudioResampler::new();
        let frame = AudioFrameBuffer {
            sample_rate: 48000,
            num_channels: 1,
            samples_per_channel: 480,
            data: vec![8000i16; 480],
        };

        let resampled = resampler.remix_and_resample(frame, 1, 24000).expect("a whole frame");

        assert_eq!(resampled.sample_rate, 24000);
        assert_eq!(resampled.num_channels, 1);
        assert_eq!(resampled.samples_per_channel, 240);
        assert_eq!(resampled.data.len(), 240);
        // The filter ramps in over the first samples, so check one well past the start.
        assert!((resampled.data[200] as i32 - 8000).abs() < 80, "got {}", resampled.data[200]);
    }

    /// A buffer shorter than it claims is refused rather than let through to the native
    /// resampler, which asserts on it.
    #[test]
    fn a_buffer_shorter_than_it_claims_is_refused() {
        let resampler = AudioResampler::new();
        let short = AudioFrameBuffer {
            sample_rate: 48000,
            num_channels: 2,
            samples_per_channel: 480,
            data: vec![0i16; 480], // 480 samples for a 960-sample claim
        };

        assert!(matches!(
            resampler.remix_and_resample(short, 1, 24000),
            Err(AudioResamplerError::MalformedBuffer(_))
        ));
    }

    /// Neither channel count may be zero: the source one would describe no samples, the
    /// destination one would divide by nothing.
    #[test]
    fn a_channel_count_of_zero_is_refused() {
        let resampler = AudioResampler::new();
        let frame = |num_channels| AudioFrameBuffer {
            sample_rate: 48000,
            num_channels,
            samples_per_channel: 480,
            data: vec![0i16; 480],
        };

        assert!(resampler.remix_and_resample(frame(0), 1, 24000).is_err());
        assert!(resampler.remix_and_resample(frame(1), 0, 24000).is_err());
    }
}
