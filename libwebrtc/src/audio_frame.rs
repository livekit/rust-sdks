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

use std::borrow::Cow;

use crate::video_frame::FrameMetadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFrameTimestamp {
    pub rtp_timestamp: u32,
}

#[derive(Debug, Clone)]
pub struct AudioFrame<'a> {
    pub data: Cow<'a, [i16]>,
    pub sample_rate: u32,
    pub num_channels: u32,
    pub samples_per_channel: u32,
    pub timestamp: Option<AudioFrameTimestamp>,
    pub frame_metadata: Option<FrameMetadata>,
}

impl AudioFrame<'_> {
    // Owned
    pub fn new(sample_rate: u32, num_channels: u32, samples_per_channel: u32) -> Self {
        Self {
            data: vec![0; (num_channels * samples_per_channel) as usize].into(),
            sample_rate,
            num_channels,
            samples_per_channel,
            timestamp: None,
            frame_metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AudioFrame;

    #[test]
    fn new_audio_frame_has_no_timestamp_or_metadata() {
        let frame = AudioFrame::new(48_000, 1, 480);

        assert!(frame.timestamp.is_none());
        assert!(frame.frame_metadata.is_none());
    }
}
