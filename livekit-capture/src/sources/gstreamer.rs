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
use gst::glib;
use gst::prelude::*;

use crate::{
    encoded::{
        h26x::{access_unit_from_annex_b, access_unit_from_h264_avc},
        ingress::EncodedAccessUnitSource,
        CodecSpecific, EncodedFrameType, EncodedRateControl, EncodedVideoCodec,
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

/// Bitrate unit used by a GStreamer encoder property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GStreamerBitrateUnit {
    /// The encoder property expects bits per second.
    BitsPerSecond,
    /// The encoder property expects kilobits per second.
    KilobitsPerSecond,
}

impl GStreamerBitrateUnit {
    fn property_value(self, target_bitrate_bps: u64) -> u64 {
        match self {
            Self::BitsPerSecond => target_bitrate_bps,
            Self::KilobitsPerSecond => target_bitrate_bps.saturating_add(999) / 1000,
        }
    }
}

/// GStreamer encoder bitrate control used by [`GStreamerAppSinkEncodedSource`].
#[derive(Debug, Clone)]
pub struct GStreamerEncoderRateControl {
    encoder: gst::Element,
    bitrate_property: String,
    bitrate_unit: GStreamerBitrateUnit,
    last_target_bitrate_bps: Option<u64>,
}

impl GStreamerEncoderRateControl {
    /// Creates bitrate control for a GStreamer encoder element.
    pub fn new(
        encoder: gst::Element,
        bitrate_property: &str,
        bitrate_unit: GStreamerBitrateUnit,
    ) -> Self {
        Self {
            encoder,
            bitrate_property: bitrate_property.to_owned(),
            bitrate_unit,
            last_target_bitrate_bps: None,
        }
    }

    fn update(&mut self, rate_control: EncodedRateControl) {
        if self.last_target_bitrate_bps == Some(rate_control.target_bitrate_bps) {
            return;
        }

        let property_value = self.bitrate_unit.property_value(rate_control.target_bitrate_bps);
        if set_integer_property(&self.encoder, &self.bitrate_property, property_value) {
            self.last_target_bitrate_bps = Some(rate_control.target_bitrate_bps);
            log::debug!(
                "updated GStreamer encoder '{}' {}={} for WebRTC target {} bps at {:.2} fps",
                self.encoder.name(),
                self.bitrate_property,
                property_value,
                rate_control.target_bitrate_bps,
                rate_control.framerate_fps,
            );
        }
    }
}

/// Encoded source backed by a GStreamer appsink.
#[derive(Debug)]
pub struct GStreamerAppSinkEncodedSource {
    appsink: gst_app::AppSink,
    config: GStreamerAppSinkConfig,
    next_fallback_timestamp_us: i64,
    rate_control: Option<GStreamerEncoderRateControl>,
}

impl GStreamerAppSinkEncodedSource {
    /// Creates an encoded source from an existing GStreamer appsink.
    pub fn new(appsink: gst_app::AppSink, config: GStreamerAppSinkConfig) -> Self {
        Self {
            appsink,
            config,
            next_fallback_timestamp_us: config.start_timestamp_us,
            rate_control: None,
        }
    }

    /// Sets the encoder bitrate control used for downstream rate requests.
    pub fn set_encoder_rate_control(&mut self, rate_control: GStreamerEncoderRateControl) {
        self.rate_control = Some(rate_control);
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

    fn request_keyframe(&mut self) {
        // The `GstForceKeyUnit` custom upstream event is understood by every
        // GStreamer video encoder (it is what gst-video's force-key-unit
        // helper builds), so downstream PLI/FIR reaches the producer.
        let structure =
            gst::Structure::builder("GstForceKeyUnit").field("all-headers", true).build();
        let _ = self.appsink.send_event(gst::event::CustomUpstream::new(structure));
    }

    fn update_rate_control(&mut self, rate_control: EncodedRateControl) {
        if let Some(control) = &mut self.rate_control {
            control.update(rate_control);
        }
    }
}

fn set_integer_property(element: &gst::Element, property: &str, value: u64) -> bool {
    let Some(pspec) = element.find_property(property) else {
        log::warn!("GStreamer encoder '{}' has no '{property}' property", element.name());
        return false;
    };

    let flags = pspec.flags();
    if !flags.contains(glib::ParamFlags::WRITABLE)
        || flags.contains(glib::ParamFlags::CONSTRUCT_ONLY)
    {
        log::warn!("GStreamer encoder '{}' property '{property}' is not writable", element.name());
        return false;
    }

    if let Some(pspec) = pspec.downcast_ref::<glib::ParamSpecUInt>() {
        element.set_property(
            property,
            value.clamp(pspec.minimum() as u64, pspec.maximum() as u64) as u32,
        );
        return true;
    }
    if let Some(pspec) = pspec.downcast_ref::<glib::ParamSpecInt>() {
        element.set_property(
            property,
            clamp_to_i64(value, pspec.minimum() as i64, pspec.maximum() as i64) as i32,
        );
        return true;
    }
    if let Some(pspec) = pspec.downcast_ref::<glib::ParamSpecUInt64>() {
        element.set_property(property, value.clamp(pspec.minimum(), pspec.maximum()));
        return true;
    }
    if let Some(pspec) = pspec.downcast_ref::<glib::ParamSpecInt64>() {
        element.set_property(property, clamp_to_i64(value, pspec.minimum(), pspec.maximum()));
        return true;
    }

    log::warn!(
        "GStreamer encoder '{}' property '{property}' has unsupported type '{}'",
        element.name(),
        pspec.value_type()
    );
    false
}

fn clamp_to_i64(value: u64, minimum: i64, maximum: i64) -> i64 {
    let value = value.min(i64::MAX as u64) as i64;
    value.clamp(minimum, maximum)
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
            access_unit.codec_specific = CodecSpecific::default_for(codec);
            Ok(access_unit)
        }
    }
}

fn clock_time_to_timestamp_us(start_timestamp_us: i64, timestamp: gst::ClockTime) -> i64 {
    let timestamp_us = timestamp.useconds().min(i64::MAX as u64) as i64;
    start_timestamp_us.saturating_add(timestamp_us)
}

/// Name of the appsink element the pipeline helpers look up or create.
pub const ENCODED_APPSINK_NAME: &str = "lk_appsink";

/// Error returned by the GStreamer pipeline helpers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GStreamerPipelineError {
    /// The requested codec does not match what the pipeline advertises.
    #[error(
        "GStreamer codec mismatch: requested {requested:?}, but {location} advertises {advertised:?}"
    )]
    CodecMismatch {
        /// Codec requested by the caller.
        requested: EncodedVideoCodec,
        /// Codec advertised by the pipeline.
        advertised: EncodedVideoCodec,
        /// Pipeline location that advertised the codec.
        location: String,
    },
    /// The pipeline has no usable appsink and no unlinked encoded pad.
    #[error(
        "GStreamer pipeline must include `appsink name={ENCODED_APPSINK_NAME}` or leave one \
         encoded video source pad unlinked"
    )]
    MissingAppSink,
    /// The named element exists but is not an appsink.
    #[error("GStreamer element {ENCODED_APPSINK_NAME} is not an appsink")]
    NotAnAppSink,
    /// Pad caps advertise no supported encoded video codec.
    #[error("unlinked GStreamer pad '{0}' does not advertise supported encoded video caps")]
    UnsupportedPadCaps(String),
    /// Caps advertise a stream layout the encoded sources cannot consume.
    #[error("unsupported GStreamer caps: {0}")]
    UnsupportedCaps(String),
    /// Element creation or linking failed.
    #[error("{0}")]
    Pipeline(String),
}

/// Returns the appsink caps for a codec as a launch-string fragment.
///
/// This is the single per-codec caps table: [`encoded_caps`] and pipeline
/// descriptions embedding a capsfilter should all derive from it.
pub fn encoded_caps_string(codec: EncodedVideoCodec) -> &'static str {
    match codec {
        EncodedVideoCodec::H264 => "video/x-h264,stream-format=byte-stream,alignment=au",
        EncodedVideoCodec::H265 => "video/x-h265,stream-format=byte-stream,alignment=au",
        EncodedVideoCodec::VP8 => "video/x-vp8",
        EncodedVideoCodec::VP9 => "video/x-vp9,profile=(string)0",
        EncodedVideoCodec::AV1 => "video/x-av1,stream-format=obu-stream,alignment=tu",
    }
}

/// Returns the appsink caps for a codec.
pub fn encoded_caps(codec: EncodedVideoCodec) -> Result<gst::Caps, GStreamerPipelineError> {
    encoded_caps_string(codec)
        .parse::<gst::Caps>()
        .map_err(|err| GStreamerPipelineError::Pipeline(format!("invalid encoded caps: {err}")))
}

/// Returns the appsink sample format used to ingest a codec.
pub fn sample_format_for_codec(codec: EncodedVideoCodec) -> GStreamerSampleFormat {
    match codec {
        EncodedVideoCodec::H264 => GStreamerSampleFormat::H264AnnexB,
        EncodedVideoCodec::H265 => GStreamerSampleFormat::H265AnnexB,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            GStreamerSampleFormat::AccessUnit { codec }
        }
    }
}

/// Returns the parser element name used to normalize a codec, when one is needed.
pub fn parser_name(codec: EncodedVideoCodec) -> Option<&'static str> {
    match codec {
        EncodedVideoCodec::H264 => Some("h264parse"),
        EncodedVideoCodec::H265 => Some("h265parse"),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 => None,
        EncodedVideoCodec::AV1 => Some("av1parse"),
    }
}

/// Finds or builds the encoded appsink in a pipeline.
///
/// When the pipeline already contains `appsink name=lk_appsink`, it is used
/// as-is (its sink caps decide the sample format). Otherwise the pipeline
/// must leave one encoded video source pad unlinked; the codec parser, a
/// capsfilter, and an appsink are created and linked to it.
pub fn ensure_encoded_appsink(
    pipeline: &gst::Pipeline,
    requested_codec: Option<EncodedVideoCodec>,
) -> Result<(gst_app::AppSink, GStreamerSampleFormat), GStreamerPipelineError> {
    if let Some(appsink) = pipeline.by_name(ENCODED_APPSINK_NAME) {
        let sample_format = match sample_format_from_element_sink_caps(&appsink)? {
            Some(sample_format) => {
                if let Some(requested_codec) = requested_codec {
                    if requested_codec != sample_format.codec() {
                        return Err(GStreamerPipelineError::CodecMismatch {
                            requested: requested_codec,
                            advertised: sample_format.codec(),
                            location: format!("appsink '{ENCODED_APPSINK_NAME}'"),
                        });
                    }
                }
                sample_format
            }
            None => sample_format_for_codec(requested_codec.unwrap_or(EncodedVideoCodec::H264)),
        };
        let appsink = appsink
            .downcast::<gst_app::AppSink>()
            .map_err(|_| GStreamerPipelineError::NotAnAppSink)?;
        return Ok((appsink, sample_format));
    }

    let src_pad = pipeline
        .find_unlinked_pad(gst::PadDirection::Src)
        .ok_or(GStreamerPipelineError::MissingAppSink)?;
    let inferred_codec = codec_from_pad_caps(&src_pad)
        .ok_or_else(|| GStreamerPipelineError::UnsupportedPadCaps(src_pad.name().to_string()))?;
    let codec = match requested_codec {
        Some(requested_codec) if requested_codec != inferred_codec => {
            return Err(GStreamerPipelineError::CodecMismatch {
                requested: requested_codec,
                advertised: inferred_codec,
                location: format!("unlinked pad '{}'", src_pad.name()),
            });
        }
        Some(requested_codec) => requested_codec,
        None => inferred_codec,
    };
    let sample_format = sample_format_for_codec(codec);
    let src_element = src_pad.parent_element().ok_or_else(|| {
        GStreamerPipelineError::Pipeline(
            "unlinked GStreamer encoded pad has no parent element".to_owned(),
        )
    })?;

    let parser = parser_element_for_codec(codec)?;
    let codec_caps = encoded_caps(codec)?;
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", codec_caps)
        .build()
        .map_err(|err| {
        GStreamerPipelineError::Pipeline(format!("failed to create {codec:?} capsfilter: {err}"))
    })?;
    let appsink = gst::ElementFactory::make("appsink")
        .name(ENCODED_APPSINK_NAME)
        .property("sync", false)
        .property("max-buffers", 8u32)
        .property("drop", true)
        .build()
        .map_err(|err| {
            GStreamerPipelineError::Pipeline(format!("failed to create appsink: {err}"))
        })?;

    if let Some(parser) = &parser {
        pipeline.add(parser).map_err(|err| {
            GStreamerPipelineError::Pipeline(format!(
                "failed to add {} to GStreamer pipeline: {err}",
                parser.name()
            ))
        })?;
    }
    pipeline.add(&capsfilter).map_err(|err| {
        GStreamerPipelineError::Pipeline(format!(
            "failed to add capsfilter to GStreamer pipeline: {err}"
        ))
    })?;
    pipeline.add(&appsink).map_err(|err| {
        GStreamerPipelineError::Pipeline(format!(
            "failed to add appsink to GStreamer pipeline: {err}"
        ))
    })?;
    if let Some(parser) = &parser {
        gst::Element::link_many([parser, &capsfilter, &appsink]).map_err(|err| {
            GStreamerPipelineError::Pipeline(format!(
                "failed to link {} to appsink: {err}",
                parser.name()
            ))
        })?;
    } else {
        gst::Element::link_many([&capsfilter, &appsink]).map_err(|err| {
            GStreamerPipelineError::Pipeline(format!("failed to link capsfilter to appsink: {err}"))
        })?;
    }
    let link_target = parser.as_ref().unwrap_or(&capsfilter);
    let sink_pad = link_target.static_pad("sink").ok_or_else(|| {
        GStreamerPipelineError::Pipeline(format!(
            "{} did not expose a sink pad",
            link_target.name()
        ))
    })?;
    src_pad.link(&sink_pad).map_err(|err| {
        GStreamerPipelineError::Pipeline(format!(
            "failed to link '{}' to {}: {err}",
            src_element.name(),
            link_target.name()
        ))
    })?;

    let appsink =
        appsink.downcast::<gst_app::AppSink>().map_err(|_| GStreamerPipelineError::NotAnAppSink)?;
    Ok((appsink, sample_format))
}

fn parser_element_for_codec(
    codec: EncodedVideoCodec,
) -> Result<Option<gst::Element>, GStreamerPipelineError> {
    let Some(name) = parser_name(codec) else {
        return Ok(None);
    };
    let mut builder = gst::ElementFactory::make(name);
    if matches!(codec, EncodedVideoCodec::H264 | EncodedVideoCodec::H265) {
        builder = builder.property("config-interval", -1i32);
    }
    builder
        .build()
        .map(Some)
        .map_err(|err| GStreamerPipelineError::Pipeline(format!("failed to create {name}: {err}")))
}

fn sample_format_from_element_sink_caps(
    element: &gst::Element,
) -> Result<Option<GStreamerSampleFormat>, GStreamerPipelineError> {
    let Some(sink_pad) = element.static_pad("sink") else {
        return Ok(None);
    };
    sample_format_from_pad_caps(&sink_pad)
}

fn sample_format_from_pad_caps(
    pad: &gst::Pad,
) -> Result<Option<GStreamerSampleFormat>, GStreamerPipelineError> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    for structure in caps.iter() {
        if let Some(sample_format) = sample_format_from_caps_structure(structure)? {
            return Ok(Some(sample_format));
        }
    }
    Ok(None)
}

/// Infers the appsink sample format from a caps structure.
pub fn sample_format_from_caps_structure(
    structure: &gst::StructureRef,
) -> Result<Option<GStreamerSampleFormat>, GStreamerPipelineError> {
    let Some(codec) = codec_from_caps_name(structure.name()) else {
        return Ok(None);
    };

    match codec {
        EncodedVideoCodec::H264 => {
            let stream_format = structure.get::<String>("stream-format").ok();
            match stream_format.as_deref() {
                Some("avc") | Some("avc3") => Ok(Some(GStreamerSampleFormat::H264Avc {
                    nal_length_size: h264_avc_nal_length_size_from_caps(structure),
                })),
                Some("byte-stream") | None => Ok(Some(GStreamerSampleFormat::H264AnnexB)),
                Some(stream_format) => Err(GStreamerPipelineError::UnsupportedCaps(format!(
                    "H.264 stream-format '{stream_format}'; expected byte-stream or avc"
                ))),
            }
        }
        EncodedVideoCodec::H265 => Ok(Some(GStreamerSampleFormat::H265AnnexB)),
        EncodedVideoCodec::VP8 => Ok(Some(GStreamerSampleFormat::AccessUnit { codec })),
        EncodedVideoCodec::VP9 => {
            let profile = structure.get::<String>("profile").ok();
            match profile.as_deref() {
                Some("0") | None => Ok(Some(GStreamerSampleFormat::AccessUnit { codec })),
                Some(profile) => Err(GStreamerPipelineError::UnsupportedCaps(format!(
                    "VP9 profile '{profile}'; expected profile 0"
                ))),
            }
        }
        EncodedVideoCodec::AV1 => {
            let stream_format = structure.get::<String>("stream-format").ok();
            match stream_format.as_deref() {
                Some("obu-stream") | None => Ok(Some(GStreamerSampleFormat::AccessUnit { codec })),
                Some(stream_format) => Err(GStreamerPipelineError::UnsupportedCaps(format!(
                    "AV1 stream-format '{stream_format}'; expected obu-stream"
                ))),
            }
        }
    }
}

fn h264_avc_nal_length_size_from_caps(structure: &gst::StructureRef) -> u8 {
    let Ok(codec_data) = structure.get::<gst::Buffer>("codec_data") else {
        return 4;
    };
    let Ok(codec_data) = codec_data.map_readable() else {
        return 4;
    };
    h264_avc_nal_length_size_from_codec_data(codec_data.as_ref()).unwrap_or(4)
}

/// Reads the AVC NAL length-prefix size from `avcC` codec data.
pub fn h264_avc_nal_length_size_from_codec_data(codec_data: &[u8]) -> Option<u8> {
    let length_size = (codec_data.get(4)? & 0x03) + 1;
    (1..=4).contains(&length_size).then_some(length_size)
}

/// Infers the encoded codec advertised by a pad's caps.
pub fn codec_from_pad_caps(pad: &gst::Pad) -> Option<EncodedVideoCodec> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.iter().find_map(|structure| codec_from_caps_name(structure.name()))
}

/// Maps a caps media-type name to an encoded codec.
pub fn codec_from_caps_name(name: &str) -> Option<EncodedVideoCodec> {
    match name {
        "video/x-h264" => Some(EncodedVideoCodec::H264),
        "video/x-h265" => Some(EncodedVideoCodec::H265),
        "video/x-vp8" => Some(EncodedVideoCodec::VP8),
        "video/x-vp9" => Some(EncodedVideoCodec::VP9),
        "video/x-av1" => Some(EncodedVideoCodec::AV1),
        _ => None,
    }
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
    fn sample_payload_access_unit_sets_vp9_and_av1_specifics() {
        let vp9 = access_unit_from_sample_payload(
            GStreamerSampleFormat::AccessUnit { codec: EncodedVideoCodec::VP9 },
            &[1, 2, 3],
            2_000,
            EncodedFrameType::Key,
            640,
            480,
        )
        .unwrap();
        assert_eq!(vp9.codec_specific, CodecSpecific::default_for(EncodedVideoCodec::VP9));

        let av1 = access_unit_from_sample_payload(
            GStreamerSampleFormat::AccessUnit { codec: EncodedVideoCodec::AV1 },
            &[1, 2, 3],
            2_000,
            EncodedFrameType::Key,
            640,
            480,
        )
        .unwrap();
        assert_eq!(av1.codec_specific, CodecSpecific::default_for(EncodedVideoCodec::AV1));
    }

    #[test]
    fn clock_time_is_offset_from_start_timestamp() {
        let timestamp = clock_time_to_timestamp_us(10_000, gst::ClockTime::from_useconds(1_234));
        assert_eq!(timestamp, 11_234);
    }
}
