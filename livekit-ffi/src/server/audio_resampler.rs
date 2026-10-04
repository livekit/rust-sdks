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
    pub fn remix_and_resample(
        &self,
        buffer: AudioFrameBuffer,
        num_channels: u32,
        sample_rate: u32,
    ) -> AudioFrameBuffer {
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

        AudioFrameBuffer {
            sample_rate,
            num_channels,
            // num_channels is foreign input, and a zero would panic the division.
            samples_per_channel: data.len() as u32 / num_channels.max(1),
            data,
        }
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

        let resampled = resampler.remix_and_resample(frame, 1, 24000);

        assert_eq!(resampled.sample_rate, 24000);
        assert_eq!(resampled.num_channels, 1);
        assert_eq!(resampled.samples_per_channel, 240);
        assert_eq!(resampled.data.len(), 240);
        // The filter ramps in over the first samples, so check one well past the start.
        assert!((resampled.data[200] as i32 - 8000).abs() < 80, "got {}", resampled.data[200]);
    }
}
