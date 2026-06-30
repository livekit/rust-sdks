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

use std::error::Error as StdError;

use bytes::Bytes;
use thiserror::Error;

use ::gstreamer as gst;
use ::gstreamer_app as gst_app;

use crate::{
    encoded::{
        h26x::{access_unit_from_annex_b, access_unit_from_h264_avc},
        ingress::EncodedAccessUnitSource,
        CodecSpecific, EncodedFrameType, EncodedVideoCodec, H264PacketizationMode,
        OwnedEncodedAccessUnit,
    },
    error::CaptureError,
};

/// Encoded sample format expected from a GStreamer appsink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GStreamerSampleFormat {
    /// H.264 Annex-B access units, usually from `h264parse` with byte-stream caps.
    H264AnnexB,
    /// H.264 access units with AVC length-prefixed NAL units.
    H264Avc {
        /// Length-prefix size in bytes.
        nal_length_size: u8,
    },
    /// H.265 Annex-B access units, usually from `h265parse` with byte-stream caps.
    H265AnnexB,
    /// One already-delimited encoded access unit per appsink sample.
    AccessUnit {
        /// Codec carried by each appsink sample.
        codec: EncodedVideoCodec,
    },
}

impl GStreamerSampleFormat {
    /// Returns the encoded codec carried by this sample format.
    pub fn codec(self) -> EncodedVideoCodec {
        match self {
            Self::H264AnnexB => EncodedVideoCodec::H264,
            Self::H264Avc { .. } => EncodedVideoCodec::H264,
            Self::H265AnnexB => EncodedVideoCodec::H265,
            Self::AccessUnit { codec } => codec,
        }
    }
}

/// Configuration for a GStreamer appsink encoded source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GStreamerAppSinkConfig {
    /// Format of encoded buffers pulled from appsink.
    pub sample_format: GStreamerSampleFormat,
    /// Timestamp added to the first buffer timestamp, or used directly as fallback.
    pub start_timestamp_us: i64,
    /// Fallback frame interval when a GStreamer buffer has no PTS or DTS.
    pub frame_interval_us: i64,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
}

impl GStreamerAppSinkConfig {
    /// Creates GStreamer appsink source configuration.
    pub fn new(
        sample_format: GStreamerSampleFormat,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        width: u32,
        height: u32,
    ) -> Self {
        Self { sample_format, start_timestamp_us, frame_interval_us, width, height }
    }
}

/// Encoded source backed by a GStreamer appsink.
#[derive(Debug)]
pub struct GStreamerAppSinkEncodedSource {
    appsink: gst_app::AppSink,
    config: GStreamerAppSinkConfig,
    next_fallback_timestamp_us: i64,
}

impl GStreamerAppSinkEncodedSource {
    /// Creates an encoded source from an existing GStreamer appsink.
    pub fn new(appsink: gst_app::AppSink, config: GStreamerAppSinkConfig) -> Self {
        Self { appsink, config, next_fallback_timestamp_us: config.start_timestamp_us }
    }

    /// Returns the wrapped appsink.
    pub fn appsink(&self) -> &gst_app::AppSink {
        &self.appsink
    }

    /// Returns the source configuration.
    pub fn config(&self) -> GStreamerAppSinkConfig {
        self.config
    }

    /// Consumes this source and returns the wrapped appsink.
    pub fn into_appsink(self) -> gst_app::AppSink {
        self.appsink
    }

    fn access_unit_from_sample(
        &mut self,
        sample: &gst::Sample,
    ) -> Result<OwnedEncodedAccessUnit, GStreamerSourceError> {
        let buffer = sample.buffer().ok_or(GStreamerSourceError::MissingBuffer)?;
        let timestamp_us = self.timestamp_us(buffer);
        let frame_type = if buffer.flags().contains(gst::BufferFlags::DELTA_UNIT) {
            EncodedFrameType::Delta
        } else {
            EncodedFrameType::Key
        };

        let map = buffer
            .map_readable()
            .map_err(|err| GStreamerSourceError::MapReadable(err.to_string()))?;
        let payload = map.as_ref();
        access_unit_from_sample_payload(
            self.config.sample_format,
            payload,
            timestamp_us,
            frame_type,
            self.config.width,
            self.config.height,
        )
        .map_err(GStreamerSourceError::Capture)
    }

    fn timestamp_us(&mut self, buffer: &gst::BufferRef) -> i64 {
        if let Some(timestamp) = buffer.pts().or_else(|| buffer.dts()) {
            let timestamp_us =
                clock_time_to_timestamp_us(self.config.start_timestamp_us, timestamp);
            self.next_fallback_timestamp_us =
                timestamp_us.saturating_add(self.config.frame_interval_us);
            return timestamp_us;
        }

        let timestamp_us = self.next_fallback_timestamp_us;
        self.next_fallback_timestamp_us =
            self.next_fallback_timestamp_us.saturating_add(self.config.frame_interval_us);
        timestamp_us
    }
}

impl EncodedAccessUnitSource for GStreamerAppSinkEncodedSource {
    type Error = GStreamerSourceError;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        match self.appsink.pull_sample() {
            Ok(sample) => self.access_unit_from_sample(&sample).map(Some),
            Err(_err) if self.appsink.is_eos() => Ok(None),
            Err(err) => Err(GStreamerSourceError::PullSample(err.to_string())),
        }
    }
}

/// Error returned by GStreamer appsink encoded sources.
#[derive(Debug, Error)]
pub enum GStreamerSourceError {
    /// The appsink failed to produce a sample.
    #[error("failed to pull GStreamer appsink sample: {0}")]
    PullSample(String),
    /// The sample did not contain an encoded buffer.
    #[error("GStreamer sample did not contain a buffer")]
    MissingBuffer,
    /// The sample buffer could not be mapped for reading.
    #[error("failed to map GStreamer buffer for reading: {0}")]
    MapReadable(String),
    /// Access-unit construction failed.
    #[error(transparent)]
    Capture(CaptureError),
}

/// Callback-backed encoded source for GStreamer appsink integrations.
#[derive(Debug)]
pub struct GStreamerAppSinkSource<F> {
    next_access_unit: F,
}

impl<F> GStreamerAppSinkSource<F> {
    /// Creates a source from a callback that pulls the next encoded appsink sample.
    pub fn new(next_access_unit: F) -> Self {
        Self { next_access_unit }
    }

    /// Returns the wrapped callback.
    pub fn callback(&self) -> &F {
        &self.next_access_unit
    }

    /// Returns the wrapped callback mutably.
    pub fn callback_mut(&mut self) -> &mut F {
        &mut self.next_access_unit
    }

    /// Consumes this source and returns the wrapped callback.
    pub fn into_callback(self) -> F {
        self.next_access_unit
    }
}

impl<F, E> EncodedAccessUnitSource for GStreamerAppSinkSource<F>
where
    F: FnMut() -> Result<Option<OwnedEncodedAccessUnit>, E>,
    E: StdError + Send + Sync + 'static,
{
    type Error = E;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        (self.next_access_unit)()
    }
}

fn access_unit_from_sample_payload(
    sample_format: GStreamerSampleFormat,
    payload: &[u8],
    timestamp_us: i64,
    frame_type: EncodedFrameType,
    width: u32,
    height: u32,
) -> Result<OwnedEncodedAccessUnit, CaptureError> {
    match sample_format {
        GStreamerSampleFormat::H264AnnexB => access_unit_from_annex_b(
            EncodedVideoCodec::H264,
            Bytes::copy_from_slice(payload),
            timestamp_us,
            width,
            height,
        ),
        GStreamerSampleFormat::H264Avc { nal_length_size } => {
            access_unit_from_h264_avc(payload, nal_length_size, timestamp_us, width, height)
        }
        GStreamerSampleFormat::H265AnnexB => access_unit_from_annex_b(
            EncodedVideoCodec::H265,
            Bytes::copy_from_slice(payload),
            timestamp_us,
            width,
            height,
        ),
        GStreamerSampleFormat::AccessUnit { codec } => {
            if payload.is_empty() {
                return Err(CaptureError::EmptyPayload);
            }

            let mut access_unit = OwnedEncodedAccessUnit::new(
                codec,
                Bytes::copy_from_slice(payload),
                timestamp_us,
                frame_type,
                width,
                height,
            );
            access_unit.codec_specific = codec_specific_for(codec);
            Ok(access_unit)
        }
    }
}

fn codec_specific_for(codec: EncodedVideoCodec) -> CodecSpecific {
    match codec {
        EncodedVideoCodec::H264 => {
            CodecSpecific::H264 { packetization_mode: H264PacketizationMode::NonInterleaved }
        }
        EncodedVideoCodec::H265 => CodecSpecific::H265,
        EncodedVideoCodec::VP8 => CodecSpecific::VP8 { temporal_id: None, layer_sync: false },
        EncodedVideoCodec::VP9 => {
            CodecSpecific::VP9 { temporal_id: None, spatial_id: None, inter_layer_predicted: None }
        }
        EncodedVideoCodec::AV1 => CodecSpecific::AV1 {
            scalability_mode: Some("L1T1".to_string()),
            dependency_descriptor: None,
        },
    }
}

fn clock_time_to_timestamp_us(start_timestamp_us: i64, timestamp: gst::ClockTime) -> i64 {
    let timestamp_us = timestamp.useconds().min(i64::MAX as u64) as i64;
    start_timestamp_us.saturating_add(timestamp_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_payload_h264_annex_b_detects_keyframe() {
        let access_unit = access_unit_from_sample_payload(
            GStreamerSampleFormat::H264AnnexB,
            &[0, 0, 1, 0x65, 1, 2],
            1_000,
            EncodedFrameType::Delta,
            640,
            480,
        )
        .unwrap();

        assert_eq!(access_unit.codec, EncodedVideoCodec::H264);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.timestamp_us, 1_000);
    }

    #[test]
    fn sample_payload_h264_avc_converts_to_annex_b_and_detects_keyframe() {
        let access_unit = access_unit_from_sample_payload(
            GStreamerSampleFormat::H264Avc { nal_length_size: 4 },
            &[0, 0, 0, 3, 0x65, 1, 2],
            1_000,
            EncodedFrameType::Delta,
            640,
            480,
        )
        .unwrap();

        assert_eq!(access_unit.codec, EncodedVideoCodec::H264);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
    }

    #[test]
    fn sample_payload_access_unit_uses_buffer_delta_flag() {
        let access_unit = access_unit_from_sample_payload(
            GStreamerSampleFormat::AccessUnit { codec: EncodedVideoCodec::VP8 },
            &[1, 2, 3],
            2_000,
            EncodedFrameType::Delta,
            640,
            480,
        )
        .unwrap();

        assert_eq!(access_unit.codec, EncodedVideoCodec::VP8);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
        assert_eq!(
            access_unit.codec_specific,
            CodecSpecific::VP8 { temporal_id: None, layer_sync: false }
        );
    }

    #[test]
    fn clock_time_is_offset_from_start_timestamp() {
        let timestamp = clock_time_to_timestamp_us(10_000, gst::ClockTime::from_useconds(1_234));
        assert_eq!(timestamp, 11_234);
    }
}
