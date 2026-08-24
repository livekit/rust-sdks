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

//! Pre-encoded video: the source trait, access units, and the pump.
//!
//! A source produces [`OwnedEncodedAccessUnit`]s, and [`EncodedVideoPump`]
//! publishes them to an RTC track as passthrough — without re-encoding.
//!
//! [`EncodedVideoSource`] is object-safe and implemented for `Box<dyn ...>`,
//! so sources constructed dynamically run through the same generic pump.

use crate::{error::SourceError, primitive::VideoResolution, pump::PumpStop};
use bytes::Bytes;
use livekit::{
    options::VideoCodec,
    webrtc::{
        video_frame::{
            EncodedFrameType as RtcEncodedFrameType, EncodedVideoCodec as RtcEncodedVideoCodec,
        },
        video_source::EncodedRateControl,
    },
};

pub mod h26x;
mod pump;
pub use pump::EncodedVideoPump;

/// Source of pre-encoded video access units, such as an encoding pipeline.
pub trait EncodedVideoSource: Send {
    /// Nominal output resolution, used to size the RTC source.
    fn resolution(&self) -> VideoResolution;

    /// Codec produced by this source. The codec is fixed for the source's
    /// lifetime.
    fn codec(&self) -> EncodedVideoCodec;

    /// Blocks until the next access unit is available. Returns `Ok(None)`
    /// at the end of the stream.
    ///
    /// Implementations must return `Ok(None)` promptly once `stop` fires:
    /// integrate the token into the blocking wait, or bound each wait to
    /// about one frame interval. The pump uses the token to tell a stop
    /// from the end of the stream.
    ///
    /// Access units must carry a non-empty payload. The pump reports an
    /// empty payload as a source error.
    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError>;

    /// Forwards a downstream keyframe request (PLI/FIR, late subscriber) to
    /// the producer so it can emit a keyframe.
    ///
    /// The default implementation does nothing, for sources that cannot
    /// influence the upstream encoder.
    fn request_keyframe(&mut self) {}

    /// Forwards a downstream rate-control target to the producer.
    ///
    /// The default implementation does nothing, for sources that cannot
    /// influence the upstream encoder.
    fn update_rate_control(&mut self, _target: EncodedRateControl) {}
}

/// Encoded video codec carried by an [`OwnedEncodedAccessUnit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum EncodedVideoCodec {
    /// H.264/AVC video.
    H264,
    /// H.265/HEVC video.
    H265,
    /// VP8 video.
    VP8,
    /// VP9 video.
    VP9,
    /// AV1 video.
    AV1,
}

/// Encoded video frame type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedFrameType {
    /// A key frame.
    Key,
    /// A delta frame.
    Delta,
}

/// Owned encoded video access unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedEncodedAccessUnit {
    /// Codec of the payload.
    pub codec: EncodedVideoCodec,
    /// Encoded payload bytes.
    pub payload: Bytes,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Frame type.
    pub frame_type: EncodedFrameType,
    /// Frame resolution in pixels.
    pub resolution: VideoResolution,
}

impl OwnedEncodedAccessUnit {
    /// Creates an access unit.
    pub fn new(
        codec: EncodedVideoCodec,
        payload: impl Into<Bytes>,
        timestamp_us: i64,
        frame_type: EncodedFrameType,
        resolution: VideoResolution,
    ) -> Self {
        Self { codec, payload: payload.into(), timestamp_us, frame_type, resolution }
    }
}

impl From<EncodedVideoCodec> for VideoCodec {
    fn from(value: EncodedVideoCodec) -> Self {
        match value {
            EncodedVideoCodec::H264 => Self::H264,
            EncodedVideoCodec::H265 => Self::H265,
            EncodedVideoCodec::VP8 => Self::VP8,
            EncodedVideoCodec::VP9 => Self::VP9,
            EncodedVideoCodec::AV1 => Self::AV1,
        }
    }
}

impl From<EncodedVideoCodec> for RtcEncodedVideoCodec {
    fn from(value: EncodedVideoCodec) -> Self {
        match value {
            EncodedVideoCodec::H264 => Self::H264,
            EncodedVideoCodec::H265 => Self::H265,
            EncodedVideoCodec::VP8 => Self::VP8,
            EncodedVideoCodec::VP9 => Self::VP9,
            EncodedVideoCodec::AV1 => Self::AV1,
        }
    }
}

impl From<EncodedFrameType> for RtcEncodedFrameType {
    fn from(value: EncodedFrameType) -> Self {
        match value {
            EncodedFrameType::Key => Self::Key,
            EncodedFrameType::Delta => Self::Delta,
        }
    }
}

impl<S: EncodedVideoSource + ?Sized> EncodedVideoSource for Box<S> {
    fn resolution(&self) -> VideoResolution {
        (**self).resolution()
    }

    fn codec(&self) -> EncodedVideoCodec {
        (**self).codec()
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        (**self).next_access_unit(stop)
    }

    fn request_keyframe(&mut self) {
        (**self).request_keyframe()
    }

    fn update_rate_control(&mut self, target: EncodedRateControl) {
        (**self).update_rate_control(target)
    }
}

// Object safety is part of this trait's contract: dynamic applications box
// sources at their edge and drive them through the same generic pumps.
const _: () = {
    fn _assert_object_safe(_: &dyn EncodedVideoSource) {}
};
