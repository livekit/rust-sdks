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

//! Copied, validated media frames at the foreign boundary.

use super::{NativeSessionError, NativeTrackKind, NativeTrackSource};
use livekit::webrtc::{
    audio_frame::AudioFrame,
    video_frame::{
        BoxVideoFrame, I420Buffer, NV12Buffer, VideoBufferType, VideoFrame, VideoRotation,
    },
};

/// Interleaved signed PCM16. This slice accepts 48 kHz, one or two channels, up to 100 ms.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeAudioFrame {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub num_channels: u32,
    pub samples_per_channel: u32,
}
impl NativeAudioFrame {
    pub(super) fn validate(&self) -> Result<(), NativeSessionError> {
        if self.sample_rate != 48_000
            || !(1..=2).contains(&self.num_channels)
            || self.samples_per_channel == 0
            || self.samples_per_channel > 4800
            || self.samples.len() != (self.samples_per_channel * self.num_channels) as usize
        {
            return Err(invalid("expected 48 kHz PCM16, mono/stereo, 1..4800 samples per channel"));
        }
        Ok(())
    }
    pub(super) fn into_core(self) -> AudioFrame<'static> {
        AudioFrame {
            data: self.samples.into(),
            sample_rate: self.sample_rate,
            num_channels: self.num_channels,
            samples_per_channel: self.samples_per_channel,
        }
    }
}
impl From<AudioFrame<'_>> for NativeAudioFrame {
    fn from(frame: AudioFrame<'_>) -> Self {
        Self {
            samples: frame.data.into_owned(),
            sample_rate: frame.sample_rate,
            num_channels: frame.num_channels,
            samples_per_channel: frame.samples_per_channel,
        }
    }
}

/// CPU pixel layout accepted by a local video source. Decoded output is always BGRA8.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum NativeVideoPixelFormat {
    Bgra,
    Nv12,
}

/// BGRA8 or NV12 bytes copied from/to a locked CVPixelBuffer (or an equivalent CPU buffer).
/// Stride includes row padding; timestamps are monotonic microseconds, rotation is 0/90/180/270.
/// No pointer or CVPixelBuffer ownership crosses UniFFI. HDR, 10-bit and zero-copy are unsupported.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeVideoFrame {
    pub format: NativeVideoPixelFormat,
    /// UV row stride for NV12; None uses the luma stride. Unused for BGRA.
    #[uniffi(default = None)]
    pub chroma_stride: Option<u32>,
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub timestamp_us: i64,
    pub rotation: u16,
}
impl NativeVideoFrame {
    pub(super) fn into_core(self) -> Result<BoxVideoFrame, NativeSessionError> {
        dimensions(self.width, self.height)?;
        let bytes_per_pixel = if self.format == NativeVideoPixelFormat::Bgra { 4 } else { 1 };
        let uv_stride = self.chroma_stride.unwrap_or(self.stride);
        let extra = if self.format == NativeVideoPixelFormat::Nv12 {
            if uv_stride < self.width || uv_stride > self.width + 4096 {
                return Err(invalid("invalid NV12 UV stride"));
            }
            uv_stride * (self.height / 2)
        } else {
            0
        };
        if self.stride < self.width * bytes_per_pixel
            || self.stride > self.width * bytes_per_pixel + 4096
            || self.data.len() != (self.stride * self.height + extra) as usize
        {
            return Err(invalid("pixel data length/row stride does not match the frame layout"));
        }
        let rotation = match self.rotation {
            0 => VideoRotation::VideoRotation0,
            90 => VideoRotation::VideoRotation90,
            180 => VideoRotation::VideoRotation180,
            270 => VideoRotation::VideoRotation270,
            _ => return Err(invalid("rotation must be 0, 90, 180 or 270")),
        };
        let buffer: livekit::webrtc::video_frame::BoxVideoBuffer = match self.format {
            NativeVideoPixelFormat::Bgra => {
                // Same libyuv conversion as the existing FFI BGRA source, using owned slices directly.
                let chroma = self.width / 2;
                let mut buffer =
                    I420Buffer::with_strides(self.width, self.height, self.width, chroma, chroma);
                let (y, u, v) = buffer.data_mut();
                imgproc::colorcvt::argb_to_i420(
                    &self.data,
                    self.stride,
                    y,
                    self.width,
                    u,
                    chroma,
                    v,
                    chroma,
                    self.width,
                    self.height,
                    false,
                );
                Box::new(buffer)
            }
            NativeVideoPixelFormat::Nv12 => {
                let mut buffer =
                    NV12Buffer::with_strides(self.width, self.height, self.stride, uv_stride);
                let (y, uv) = buffer.data_mut();
                let (input_y, input_uv) = self.data.split_at((self.stride * self.height) as usize);
                y.copy_from_slice(input_y);
                uv.copy_from_slice(input_uv);
                Box::new(buffer)
            }
        };
        Ok(VideoFrame { rotation, timestamp_us: self.timestamp_us, frame_metadata: None, buffer })
    }

    pub(super) fn from_core(frame: BoxVideoFrame) -> Result<Self, NativeSessionError> {
        // Decoded frames may be odd-sized or portrait. Bound the BGRA allocation,
        // not the local capture format; conversion below handles chroma rounding.
        let bytes =
            frame.buffer.width().checked_mul(frame.buffer.height()).and_then(|n| n.checked_mul(4));
        if frame.buffer.width() == 0
            || frame.buffer.height() == 0
            || bytes.is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err(invalid("decoded video must be nonempty and at most 64 MiB BGRA"));
        }
        if !matches!(
            frame.buffer.buffer_type(),
            VideoBufferType::Native
                | VideoBufferType::I420
                | VideoBufferType::I420A
                | VideoBufferType::NV12
        ) {
            return Err(invalid("decoded pixel format is outside the supported 8-bit formats"));
        }
        let (data, info) = crate::server::colorcvt::to_video_buffer_info(
            frame.buffer,
            Some(crate::proto::VideoBufferType::Bgra),
            true,
        )
        .map_err(|e| invalid(&e.to_string()))?;
        Ok(Self {
            format: NativeVideoPixelFormat::Bgra,
            chroma_stride: None,
            data: data.into_vec(),
            width: info.width,
            height: info.height,
            stride: info.stride.unwrap_or(info.width * 4),
            timestamp_us: frame.timestamp_us,
            rotation: frame.rotation as u16,
        })
    }
}

/// A value snapshot; mutating the foreign record does not alter the native track.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeMediaTrackInfo {
    pub sid: String,
    pub name: String,
    pub kind: NativeTrackKind,
    pub source: NativeTrackSource,
    pub is_muted: bool,
    pub is_enabled: bool,
    pub is_remote: bool,
}
impl From<&livekit::track::Track> for NativeMediaTrackInfo {
    fn from(track: &livekit::track::Track) -> Self {
        Self {
            sid: track.sid().into(),
            name: track.name(),
            kind: track.kind(),
            source: track.source(),
            is_muted: track.is_muted(),
            is_enabled: track.is_enabled(),
            is_remote: track.is_remote(),
        }
    }
}

pub(super) fn dimensions(width: u32, height: u32) -> Result<(), NativeSessionError> {
    if !(2..=4096).contains(&width)
        || !(2..=2160).contains(&height)
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
    {
        return Err(invalid("video dimensions must be even, 2..4096 by 2..2160"));
    }
    Ok(())
}
pub(super) fn invalid(reason: &str) -> NativeSessionError {
    NativeSessionError::InvalidState { reason: reason.to_owned() }
}
