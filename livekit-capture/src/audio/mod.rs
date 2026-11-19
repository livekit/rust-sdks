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

//! Uncompressed PCM audio capture.

mod pump;

pub mod sources;

use crate::{error::SourceError, pump::PumpStop};
use std::borrow::Cow;

pub use pump::AudioPump;

/// Fixed PCM format used by an audio source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    /// Samples per second in each channel.
    pub sample_rate: u32,
    /// Number of interleaved channels.
    pub num_channels: u32,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self { sample_rate: 48_000, num_channels: 1 }
    }
}

impl AudioFormat {
    /// Creates a PCM format whose frames contain exactly 10 ms of audio.
    ///
    /// Returns `None` for zero channels or a sample rate that does not form
    /// a whole, non-zero number of samples per 10 ms frame.
    pub fn new(sample_rate: u32, num_channels: u32) -> Option<Self> {
        (sample_rate > 0 && sample_rate.is_multiple_of(100) && num_channels > 0)
            .then_some(Self { sample_rate, num_channels })
    }

    /// Returns the number of samples per channel in each 10 ms frame.
    pub fn samples_per_channel(self) -> u32 {
        self.sample_rate / 100
    }

    /// Returns the number of interleaved samples in one frame.
    pub fn samples_per_frame(self) -> usize {
        self.samples_per_channel() as usize * self.num_channels as usize
    }

    /// Returns the duration of one frame in nanoseconds.
    pub fn frame_duration_ns(self) -> u64 {
        10_000_000
    }
}

/// One owned or borrowed interleaved signed 16-bit PCM frame.
#[derive(Debug, Clone)]
pub struct PcmAudioFrame<'a> {
    /// Interleaved signed 16-bit samples.
    pub data: Cow<'a, [i16]>,
    /// Stream-relative presentation timestamp in nanoseconds.
    pub timestamp_ns: u64,
}

impl PcmAudioFrame<'_> {
    /// Converts this frame into owned storage.
    pub fn into_owned(self) -> PcmAudioFrame<'static> {
        PcmAudioFrame { data: Cow::Owned(self.data.into_owned()), timestamp_ns: self.timestamp_ns }
    }
}

/// Source of fixed-format, 10 ms interleaved signed 16-bit PCM frames.
pub trait PcmAudioSource: Send {
    /// Returns the immutable stream format, validated with [`AudioFormat::new`].
    fn format(&self) -> AudioFormat;

    /// Blocks until the next frame is available, returning `None` at EOS.
    fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<PcmAudioFrame<'static>>, SourceError>;
}

impl<S: PcmAudioSource + ?Sized> PcmAudioSource for Box<S> {
    fn format(&self) -> AudioFormat {
        (**self).format()
    }

    fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<PcmAudioFrame<'static>>, SourceError> {
        (**self).next_frame(stop)
    }
}

const _: () = {
    fn _assert_object_safe(_: &dyn PcmAudioSource) {}
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_format_is_ten_ms_48k_mono() {
        let format = AudioFormat::default();
        assert_eq!(format.samples_per_channel(), 480);
        assert_eq!(format.samples_per_frame(), 480);
        assert_eq!(format.frame_duration_ns(), 10_000_000);
    }

    #[test]
    fn format_rejects_fractional_frames() {
        assert!(AudioFormat::new(44_101, 2).is_none());
        assert!(AudioFormat::new(48_000, 0).is_none());
        assert!(AudioFormat::new(0, 1).is_none());
        let stereo = AudioFormat::new(44_100, 2).unwrap();
        assert_eq!(stereo.samples_per_channel(), 441);
        assert_eq!(stereo.samples_per_frame(), 882);
    }
}
