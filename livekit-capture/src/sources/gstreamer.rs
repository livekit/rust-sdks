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

//! Encoded video capture from a GStreamer pipeline.
//!
//! [`GStreamerVideoSource`] owns a pipeline that ends in an appsink and
//! yields the pipeline's encoded output as access units.

use ::gstreamer as gst;
use ::gstreamer_app as gst_app;
use bytes::Bytes;
use gst::glib;
use gst::prelude::*;
use thiserror::Error;

use crate::{
    encoded::{
        h26x::{access_unit_from_annex_b, access_unit_from_h264_avc, H26xParseError},
        EncodedFrameType, EncodedVideoCodec, EncodedVideoSource, OwnedEncodedAccessUnit,
    },
    error::SourceError,
    primitive::VideoResolution,
    pump::PumpStop,
};
use livekit::webrtc::video_source::EncodedRateControl;

/// Encoded sample format expected from a GStreamer appsink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GStreamerSampleFormat {
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
    fn codec(self) -> EncodedVideoCodec {
        match self {
            Self::H264AnnexB => EncodedVideoCodec::H264,
            Self::H264Avc { .. } => EncodedVideoCodec::H264,
            Self::H265AnnexB => EncodedVideoCodec::H265,
            Self::AccessUnit { codec } => codec,
        }
    }
}

/// Configuration for a GStreamer encoded video source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerVideoSourceConfig {
    /// GStreamer launch description for the encoded producer pipeline.
    ///
    /// The pipeline must contain `appsink name=lk_appsink`, or leave exactly
    /// one encoded video source pad unlinked. The source then attaches an
    /// appsink to that pad.
    pub pipeline: String,

    /// Codec expected from the pipeline. When omitted, the codec is
    /// inferred from the pipeline caps.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub codec: Option<EncodedVideoCodec>,

    /// Encoded frame resolution.
    ///
    /// When omitted, the resolution is discovered from the first sample, so
    /// construction waits for the pipeline to produce data. When set,
    /// construction returns without waiting, and the first sample is
    /// verified against this value.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub resolution: Option<VideoResolution>,

    /// Forwards WebRTC rate-control targets to an encoder element's bitrate
    /// property. Without this, the pipeline encodes at a fixed bitrate.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub rate_control: Option<GStreamerRateControlConfig>,
}

/// Binding from WebRTC rate-control targets to a GStreamer encoder property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerRateControlConfig {
    /// Name of the encoder element in the pipeline (for example
    /// `lk_encoder`).
    pub element: String,

    /// Bitrate property to set on the element (for example `bitrate` for
    /// x264enc, or `target-bitrate` for vp8enc/vp9enc).
    pub property: String,

    /// Unit the property expects.
    pub unit: GStreamerBitrateUnit,
}

/// Bitrate unit used by a GStreamer encoder property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum GStreamerBitrateUnit {
    /// The encoder property expects bits per second.
    #[cfg_attr(feature = "serde", serde(rename = "bps"))]
    BitsPerSecond,
    /// The encoder property expects kilobits per second.
    #[cfg_attr(feature = "serde", serde(rename = "kbps"))]
    KilobitsPerSecond,
}

impl GStreamerBitrateUnit {
    fn property_value(self, target_bitrate_bps: u64) -> u64 {
        match self {
            Self::BitsPerSecond => target_bitrate_bps,
            Self::KilobitsPerSecond => target_bitrate_bps.div_ceil(1000),
        }
    }
}

/// GStreamer encoder bitrate control used by [`GStreamerVideoSource`].
#[derive(Debug, Clone)]
struct GStreamerEncoderRateControl {
    encoder: gst::Element,
    bitrate_property: String,
    bitrate_unit: GStreamerBitrateUnit,
    last_target_bitrate_bps: Option<u64>,
}

impl GStreamerEncoderRateControl {
    /// Creates bitrate control for a GStreamer encoder element.
    fn new(
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

/// How long one appsink wait may block before the stop token is rechecked.
const SAMPLE_WAIT: gst::ClockTime = gst::ClockTime::from_mseconds(100);

/// How long stream discovery waits for the pipeline's first sample.
const DISCOVERY_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);

/// Fallback frame interval when neither caps nor buffers carry timing.
const DEFAULT_FRAME_INTERVAL_US: i64 = 1_000_000 / 30;

/// Encoded source that owns a GStreamer pipeline that ends in an appsink.
#[derive(Debug)]
pub struct GStreamerVideoSource {
    pipeline: gst::Pipeline,
    bus: gst::Bus,
    appsink: gst_app::AppSink,
    sample_format: GStreamerSampleFormat,
    resolution: VideoResolution,
    frame_interval_us: i64,
    next_fallback_timestamp_us: i64,
    rate_control: Option<GStreamerEncoderRateControl>,
    // Caps the stream was validated against; a pointer change on a later
    // sample triggers revalidation.
    negotiated_caps: Option<gst::Caps>,
    // Sample pulled during stream discovery, handed out first.
    pending_sample: Option<gst::Sample>,
}

impl GStreamerVideoSource {
    /// Creates the source. Construction and stream discovery run on the
    /// tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`GStreamerVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: GStreamerVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Builds and starts the GStreamer pipeline from the configuration.
    ///
    /// The pipeline starts to play immediately and returns to `Null` when
    /// the source is dropped. Construction fails on an invalid launch
    /// description, a missing appsink or encoded pad, a missing
    /// rate-control element, or a pipeline that does not start.
    ///
    /// When the configuration declares no resolution, this blocks until the
    /// first sample arrives (bounded by a timeout) to read the stream
    /// settings.
    pub fn new_blocking(config: GStreamerVideoSourceConfig) -> Result<Self, SourceError> {
        gst::init().map_err(|err| {
            SourceError::new(GStreamerVideoSourceError::Pipeline(format!(
                "failed to initialize GStreamer: {err}"
            )))
        })?;

        let pipeline = gst::parse::launch(&config.pipeline)
            .map_err(|err| {
                SourceError::new(GStreamerVideoSourceError::Pipeline(format!(
                    "failed to create pipeline: {err}"
                )))
            })?
            .downcast::<gst::Pipeline>()
            .map_err(|_| SourceError::new(GStreamerVideoSourceError::NotAPipeline))?;

        let (appsink, sample_format) = ensure_encoded_appsink(&pipeline, config.codec)
            .map_err(|err| SourceError::new(GStreamerVideoSourceError::Layout(err)))?;

        let rate_control = config
            .rate_control
            .map(|binding| -> Result<GStreamerEncoderRateControl, GStreamerVideoSourceError> {
                let encoder = pipeline.by_name(&binding.element).ok_or_else(|| {
                    GStreamerVideoSourceError::MissingRateControlElement(binding.element.clone())
                })?;
                Ok(GStreamerEncoderRateControl::new(encoder, &binding.property, binding.unit))
            })
            .transpose()
            .map_err(SourceError::new)?;

        let bus = pipeline.bus().ok_or_else(|| {
            SourceError::new(GStreamerVideoSourceError::Pipeline(
                "pipeline has no message bus".to_owned(),
            ))
        })?;

        pipeline.set_state(gst::State::Playing).map_err(|err| {
            SourceError::new(GStreamerVideoSourceError::Pipeline(format!(
                "failed to start pipeline: {err}"
            )))
        })?;

        let mut source = Self {
            pipeline,
            bus,
            appsink,
            sample_format,
            resolution: config.resolution.unwrap_or_default(),
            frame_interval_us: DEFAULT_FRAME_INTERVAL_US,
            next_fallback_timestamp_us: 0,
            rate_control,
            negotiated_caps: None,
            pending_sample: None,
        };

        // Without a declared resolution, discover the stream settings from
        // the first sample's negotiated caps; the sample is buffered so no
        // keyframe is lost. A declared resolution skips the wait and is
        // verified lazily against the first sample instead.
        if config.resolution.is_none() {
            let sample = source.wait_first_sample().map_err(SourceError::new)?;
            let caps = sample
                .caps()
                .ok_or(GStreamerVideoSourceError::MissingResolutionCaps)
                .map_err(SourceError::new)?;
            source.resolution = resolution_from_caps(caps)
                .ok_or(GStreamerVideoSourceError::MissingResolutionCaps)
                .map_err(SourceError::new)?;
            if let Some(frame_interval_us) = frame_interval_from_caps(caps) {
                source.frame_interval_us = frame_interval_us;
            }
            source.negotiated_caps = Some(caps.to_owned());
            source.pending_sample = Some(sample);
        }

        log::info!(
            "GStreamer pipeline ready: {:?} {} ({} resolution)",
            source.sample_format.codec(),
            source.resolution,
            if config.resolution.is_none() { "discovered" } else { "declared" },
        );
        Ok(source)
    }

    /// Blocks until the pipeline produces its first sample, a bus error
    /// arrives, or the discovery timeout expires.
    fn wait_first_sample(&self) -> Result<gst::Sample, GStreamerVideoSourceError> {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(DISCOVERY_TIMEOUT.seconds());
        loop {
            self.check_bus()?;
            if let Some(sample) = self.appsink.try_pull_sample(SAMPLE_WAIT) {
                return Ok(sample);
            }
            if self.appsink.is_eos() {
                return Err(GStreamerVideoSourceError::EndedBeforeFirstSample);
            }
            if std::time::Instant::now() >= deadline {
                return Err(GStreamerVideoSourceError::DiscoveryTimeout);
            }
        }
    }

    /// Returns the GStreamer pipeline.
    pub fn pipeline(&self) -> &gst::Pipeline {
        &self.pipeline
    }

    /// Returns a pending pipeline bus error, if any.
    fn check_bus(&self) -> Result<(), GStreamerVideoSourceError> {
        while let Some(message) = self.bus.pop_filtered(&[gst::MessageType::Error]) {
            if let gst::MessageView::Error(error) = message.view() {
                return Err(GStreamerVideoSourceError::Pipeline(format!(
                    "{} ({})",
                    error.error(),
                    error.debug().map(|s| s.to_string()).unwrap_or_default(),
                )));
            }
        }
        Ok(())
    }

    /// Validates a sample's caps against the established stream settings.
    fn check_caps(&mut self, sample: &gst::Sample) -> Result<(), GStreamerVideoSourceError> {
        let Some(caps) = sample.caps() else {
            return Ok(());
        };
        // Caps are immutable and refcounted, so an unchanged stream passes
        // with a pointer comparison. On a caps change, the resolution and
        // codec must match: live stream reconfiguration would require
        // republishing the track, which is not supported yet.
        if let Some(seen) = &self.negotiated_caps {
            if seen.as_ptr() == caps.as_ptr() {
                return Ok(());
            }
        }
        let had_baseline = self.negotiated_caps.is_some();

        if let Some(structure) = caps.structure(0) {
            if let Some(codec) = codec_from_caps_name(structure.name()) {
                if codec != self.sample_format.codec() {
                    return Err(GStreamerVideoSourceError::Renegotiated {
                        from: format!("{:?}", self.sample_format.codec()),
                        to: format!("{codec:?}"),
                    });
                }
            }
        }
        if let Some(resolution) = resolution_from_caps(caps) {
            if resolution != self.resolution {
                return Err(if had_baseline {
                    GStreamerVideoSourceError::Renegotiated {
                        from: self.resolution.to_string(),
                        to: resolution.to_string(),
                    }
                } else {
                    GStreamerVideoSourceError::ResolutionMismatch {
                        configured: self.resolution,
                        actual: resolution,
                    }
                });
            }
        }
        if let Some(frame_interval_us) = frame_interval_from_caps(caps) {
            self.frame_interval_us = frame_interval_us;
        }

        self.negotiated_caps = Some(caps.to_owned());
        Ok(())
    }

    fn process_sample(
        &mut self,
        sample: &gst::Sample,
    ) -> Result<OwnedEncodedAccessUnit, GStreamerVideoSourceError> {
        self.check_caps(sample)?;
        self.access_unit_from_sample(sample)
    }

    fn access_unit_from_sample(
        &mut self,
        sample: &gst::Sample,
    ) -> Result<OwnedEncodedAccessUnit, GStreamerVideoSourceError> {
        let buffer = sample.buffer().ok_or(GStreamerVideoSourceError::MissingBuffer)?;
        let timestamp_us = self.timestamp_us(buffer);
        let frame_type = if buffer.flags().contains(gst::BufferFlags::DELTA_UNIT) {
            EncodedFrameType::Delta
        } else {
            EncodedFrameType::Key
        };

        let map = buffer
            .map_readable()
            .map_err(|err| GStreamerVideoSourceError::MapReadable(err.to_string()))?;
        let payload = map.as_ref();
        access_unit_from_sample_payload(
            self.sample_format,
            payload,
            timestamp_us,
            frame_type,
            self.resolution,
        )
        .map_err(GStreamerVideoSourceError::Parse)
    }

    fn timestamp_us(&mut self, buffer: &gst::BufferRef) -> i64 {
        if let Some(timestamp) = buffer.pts().or_else(|| buffer.dts()) {
            let timestamp_us = clock_time_to_timestamp_us(0, timestamp);
            self.next_fallback_timestamp_us = timestamp_us.saturating_add(self.frame_interval_us);
            return timestamp_us;
        }

        let timestamp_us = self.next_fallback_timestamp_us;
        self.next_fallback_timestamp_us =
            self.next_fallback_timestamp_us.saturating_add(self.frame_interval_us);
        timestamp_us
    }
}

impl Drop for GStreamerVideoSource {
    fn drop(&mut self) {
        // Returning the pipeline to `Null` releases its resources; GStreamer
        // does not stop a running pipeline on the last unref.
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

impl EncodedVideoSource for GStreamerVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.resolution
    }

    fn codec(&self) -> EncodedVideoCodec {
        self.sample_format.codec()
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        if let Some(sample) = self.pending_sample.take() {
            return self.process_sample(&sample).map(Some).map_err(SourceError::new);
        }

        // Bounded waits keep the stop token observed within `SAMPLE_WAIT`
        // even while the pipeline produces nothing.
        loop {
            if stop.is_stopped() {
                return Ok(None);
            }
            self.check_bus().map_err(SourceError::new)?;

            match self.appsink.try_pull_sample(SAMPLE_WAIT) {
                Some(sample) => {
                    return self.process_sample(&sample).map(Some).map_err(SourceError::new);
                }
                None if self.appsink.is_eos() => return Ok(None),
                None => {}
            }
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
pub enum GStreamerVideoSourceError {
    /// The launch description did not produce a pipeline.
    #[error("GStreamer description did not create a pipeline")]
    NotAPipeline,
    /// The pipeline produced no data during stream discovery.
    #[error(
        "pipeline produced no data during stream discovery; declare `resolution` in the \
         configuration to skip discovery, or check that the pipeline produces encoded video"
    )]
    DiscoveryTimeout,
    /// The stream ended before producing a sample.
    #[error("pipeline reached end of stream before producing a sample")]
    EndedBeforeFirstSample,
    /// Negotiated caps carry no resolution to discover.
    #[error("negotiated caps declare no resolution; declare `resolution` in the configuration")]
    MissingResolutionCaps,
    /// The pipeline produces a different resolution than configured.
    #[error("pipeline produces {actual}, but the configuration declares {configured}")]
    ResolutionMismatch {
        /// Resolution declared in the configuration.
        configured: VideoResolution,
        /// Resolution the pipeline negotiated.
        actual: VideoResolution,
    },
    /// Stream settings changed mid-stream.
    #[error(
        "pipeline renegotiated {from} to {to}; changing stream settings requires republishing \
         the track, which is not supported yet"
    )]
    Renegotiated {
        /// Established stream setting.
        from: String,
        /// Newly negotiated stream setting.
        to: String,
    },
    /// The rate-control element is missing from the pipeline.
    #[error("pipeline has no element named '{0}' for rate control")]
    MissingRateControlElement(String),
    /// The pipeline could not be built or started, or errored at runtime.
    #[error("GStreamer pipeline error: {0}")]
    Pipeline(String),
    /// The pipeline layout cannot feed an encoded appsink.
    #[error(transparent)]
    Layout(#[from] GStreamerPipelineError),
    /// The sample did not contain an encoded buffer.
    #[error("GStreamer sample did not contain a buffer")]
    MissingBuffer,
    /// The sample buffer could not be mapped for reading.
    #[error("failed to map GStreamer buffer for reading: {0}")]
    MapReadable(String),
    /// Access-unit parsing failed.
    #[error(transparent)]
    Parse(H26xParseError),
}

fn access_unit_from_sample_payload(
    sample_format: GStreamerSampleFormat,
    payload: &[u8],
    timestamp_us: i64,
    frame_type: EncodedFrameType,
    resolution: VideoResolution,
) -> Result<OwnedEncodedAccessUnit, H26xParseError> {
    match sample_format {
        GStreamerSampleFormat::H264AnnexB => access_unit_from_annex_b(
            EncodedVideoCodec::H264,
            Bytes::copy_from_slice(payload),
            timestamp_us,
            resolution,
        ),
        GStreamerSampleFormat::H264Avc { nal_length_size } => {
            access_unit_from_h264_avc(payload, nal_length_size, timestamp_us, resolution)
        }
        GStreamerSampleFormat::H265AnnexB => access_unit_from_annex_b(
            EncodedVideoCodec::H265,
            Bytes::copy_from_slice(payload),
            timestamp_us,
            resolution,
        ),
        GStreamerSampleFormat::AccessUnit { codec } => {
            if payload.is_empty() {
                return Err(H26xParseError::EmptyPayload);
            }

            Ok(OwnedEncodedAccessUnit::new(
                codec,
                Bytes::copy_from_slice(payload),
                timestamp_us,
                frame_type,
                resolution,
            ))
        }
    }
}

/// Reads the frame resolution from negotiated caps, when declared.
fn resolution_from_caps(caps: &gst::CapsRef) -> Option<VideoResolution> {
    let structure = caps.structure(0)?;
    let width = structure.get::<i32>("width").ok()?;
    let height = structure.get::<i32>("height").ok()?;
    (width > 0 && height > 0).then(|| VideoResolution::new(width as u32, height as u32))
}

/// Derives the fallback frame interval from the caps framerate, when
/// declared and non-zero.
fn frame_interval_from_caps(caps: &gst::CapsRef) -> Option<i64> {
    let framerate = caps.structure(0)?.get::<gst::Fraction>("framerate").ok()?;
    let (numer, denom) = (i64::from(framerate.numer()), i64::from(framerate.denom()));
    (numer > 0 && denom > 0).then(|| 1_000_000 * denom / numer)
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
fn sample_format_for_codec(codec: EncodedVideoCodec) -> GStreamerSampleFormat {
    match codec {
        EncodedVideoCodec::H264 => GStreamerSampleFormat::H264AnnexB,
        EncodedVideoCodec::H265 => GStreamerSampleFormat::H265AnnexB,
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            GStreamerSampleFormat::AccessUnit { codec }
        }
    }
}

/// Returns the GStreamer parser element name for a codec, when one is
/// needed.
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
fn ensure_encoded_appsink(
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
fn sample_format_from_caps_structure(
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
fn h264_avc_nal_length_size_from_codec_data(codec_data: &[u8]) -> Option<u8> {
    let length_size = (codec_data.get(4)? & 0x03) + 1;
    (1..=4).contains(&length_size).then_some(length_size)
}

/// Infers the encoded codec advertised by a pad's caps.
fn codec_from_pad_caps(pad: &gst::Pad) -> Option<EncodedVideoCodec> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.iter().find_map(|structure| codec_from_caps_name(structure.name()))
}

/// Maps a caps media-type name to an encoded codec.
fn codec_from_caps_name(name: &str) -> Option<EncodedVideoCodec> {
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
            VideoResolution::new(640, 480),
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
            VideoResolution::new(640, 480),
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
            VideoResolution::new(640, 480),
        )
        .unwrap();

        assert_eq!(access_unit.codec, EncodedVideoCodec::VP8);
        assert_eq!(access_unit.frame_type, EncodedFrameType::Delta);
    }

    #[test]
    fn clock_time_is_offset_from_start_timestamp() {
        let timestamp = clock_time_to_timestamp_us(10_000, gst::ClockTime::from_useconds(1_234));
        assert_eq!(timestamp, 11_234);
    }
}
