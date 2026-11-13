// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.

//! PCM S16LE audio capture from a GStreamer appsink.

use crate::{
    audio::{AudioFormat, PcmAudioFrame, PcmAudioSource},
    error::SourceError,
    pump::PumpStop,
};
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use std::{borrow::Cow, collections::VecDeque};
use thiserror::Error;

const APPSINK_NAME: &str = "lk_audio_appsink";
const SAMPLE_WAIT: gst::ClockTime = gst::ClockTime::from_mseconds(50);

/// Configuration for PCM audio ingest from GStreamer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerAudioSourceConfig {
    /// Launch description containing `appsink name=lk_audio_appsink`.
    pub pipeline: String,
    /// Required output sample rate.
    pub sample_rate: u32,
    /// Required number of interleaved channels.
    pub num_channels: u32,
    /// Output frame duration in milliseconds.
    pub frame_duration_ms: u32,
}

impl Default for GStreamerAudioSourceConfig {
    fn default() -> Self {
        Self {
            pipeline: String::new(),
            sample_rate: 48_000,
            num_channels: 1,
            frame_duration_ms: 10,
        }
    }
}

/// Fixed-format GStreamer PCM source.
#[derive(Debug)]
pub struct GStreamerAudioSource {
    pipeline: gst::Pipeline,
    bus: gst::Bus,
    appsink: gst_app::AppSink,
    format: AudioFormat,
    queued_samples: VecDeque<i16>,
    next_timestamp_ns: Option<u64>,
}

impl GStreamerAudioSource {
    /// Builds and starts the configured pipeline.
    pub fn new(config: GStreamerAudioSourceConfig) -> Result<Self, GStreamerAudioSourceError> {
        gst::init().map_err(|error| GStreamerAudioSourceError::Pipeline(error.to_string()))?;
        let format = AudioFormat::with_frame_duration(
            config.sample_rate,
            config.num_channels,
            config.frame_duration_ms,
        )
        .ok_or(GStreamerAudioSourceError::InvalidFormat)?;
        let pipeline = gst::parse::launch(&config.pipeline)
            .map_err(|error| GStreamerAudioSourceError::Pipeline(error.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| GStreamerAudioSourceError::NotAPipeline)?;
        let appsink = pipeline
            .by_name(APPSINK_NAME)
            .ok_or(GStreamerAudioSourceError::MissingAppSink)?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| GStreamerAudioSourceError::NotAnAppSink)?;
        let caps: gst::Caps = format!(
            "audio/x-raw,format=S16LE,layout=interleaved,rate={},channels={}",
            format.sample_rate, format.num_channels
        )
        .parse()
        .map_err(|error| GStreamerAudioSourceError::Pipeline(format!("invalid caps: {error}")))?;
        appsink.set_caps(Some(&caps));
        appsink.set_property("max-buffers", 8u32);
        appsink.set_property("drop", true);
        appsink.set_property("sync", false);
        let bus = pipeline
            .bus()
            .ok_or_else(|| GStreamerAudioSourceError::Pipeline("pipeline has no bus".into()))?;
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| GStreamerAudioSourceError::Pipeline(error.to_string()))?;
        Ok(Self {
            pipeline,
            bus,
            appsink,
            format,
            queued_samples: VecDeque::with_capacity(format.samples_per_frame() * 2),
            next_timestamp_ns: None,
        })
    }

    fn check_bus(&self) -> Result<(), GStreamerAudioSourceError> {
        while let Some(message) = self.bus.pop_filtered(&[gst::MessageType::Error]) {
            if let gst::MessageView::Error(error) = message.view() {
                return Err(GStreamerAudioSourceError::Pipeline(format!(
                    "{} ({})",
                    error.error(),
                    error.debug().map(|value| value.to_string()).unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    fn queue_sample(&mut self, sample: gst::Sample) -> Result<(), GStreamerAudioSourceError> {
        let buffer = sample.buffer().ok_or(GStreamerAudioSourceError::MissingBuffer)?;
        if self.queued_samples.is_empty() {
            if let Some(pts) = buffer.pts() {
                self.next_timestamp_ns = Some(pts.nseconds());
            } else if self.next_timestamp_ns.is_none() {
                self.next_timestamp_ns = Some(0);
            }
        }
        let map = buffer
            .map_readable()
            .map_err(|error| GStreamerAudioSourceError::Map(error.to_string()))?;
        let bytes = map.as_ref();
        if bytes.len() % 2 != 0 {
            return Err(GStreamerAudioSourceError::OddBufferLength(bytes.len()));
        }
        self.queued_samples
            .extend(bytes.chunks_exact(2).map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]])));
        Ok(())
    }
}

impl PcmAudioSource for GStreamerAudioSource {
    fn format(&self) -> AudioFormat {
        self.format
    }

    fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<PcmAudioFrame<'static>>, SourceError> {
        while self.queued_samples.len() < self.format.samples_per_frame() {
            if stop.is_stopped() {
                return Ok(None);
            }
            self.check_bus().map_err(SourceError::new)?;
            match self.appsink.try_pull_sample(SAMPLE_WAIT) {
                Some(sample) => self.queue_sample(sample).map_err(SourceError::new)?,
                None if self.appsink.is_eos() && self.queued_samples.is_empty() => return Ok(None),
                None if self.appsink.is_eos() => {
                    return Err(SourceError::new(GStreamerAudioSourceError::IncompleteFinalFrame {
                        expected: self.format.samples_per_frame(),
                        actual: self.queued_samples.len(),
                    }))
                }
                None => continue,
            }
        }

        let data = self.queued_samples.drain(..self.format.samples_per_frame()).collect::<Vec<_>>();
        let timestamp_ns = self.next_timestamp_ns.unwrap_or(0);
        self.next_timestamp_ns = Some(timestamp_ns.saturating_add(self.format.frame_duration_ns()));
        Ok(Some(PcmAudioFrame { data: Cow::Owned(data), timestamp_ns }))
    }
}

impl Drop for GStreamerAudioSource {
    fn drop(&mut self) {
        let _ = self.pipeline.send_event(gst::event::Eos::new());
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Error constructing or reading a GStreamer PCM source.
#[derive(Debug, Error)]
pub enum GStreamerAudioSourceError {
    /// The requested PCM format is invalid.
    #[error("sample rate, channel count, and frame duration must form whole non-zero frames")]
    InvalidFormat,
    /// The launch description did not create a pipeline.
    #[error("GStreamer description did not create a pipeline")]
    NotAPipeline,
    /// The required named appsink is missing.
    #[error("pipeline must contain `appsink name={APPSINK_NAME}`")]
    MissingAppSink,
    /// The named element is not an appsink.
    #[error("pipeline element `{APPSINK_NAME}` is not an appsink")]
    NotAnAppSink,
    /// A sample had no buffer.
    #[error("GStreamer sample contained no buffer")]
    MissingBuffer,
    /// PCM bytes could not be mapped.
    #[error("failed to map PCM buffer: {0}")]
    Map(String),
    /// An S16LE buffer contained a partial sample.
    #[error("S16LE buffer length {0} is not divisible by two")]
    OddBufferLength(usize),
    /// EOS arrived with less than one complete output frame.
    #[error("final PCM frame contains {actual} samples; expected {expected}")]
    IncompleteFinalFrame {
        /// Required interleaved sample count.
        expected: usize,
        /// Received interleaved sample count.
        actual: usize,
    },
    /// Pipeline construction or execution failed.
    #[error("GStreamer audio pipeline error: {0}")]
    Pipeline(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_pipeline_yields_fixed_ten_ms_frames_and_eos() {
        let mut source = GStreamerAudioSource::new(GStreamerAudioSourceConfig {
            pipeline: "audiotestsrc wave=sine num-buffers=3 samplesperbuffer=480 ! \
                       audio/x-raw,format=S16LE,layout=interleaved,rate=48000,channels=1 ! \
                       appsink name=lk_audio_appsink"
                .into(),
            ..Default::default()
        })
        .unwrap();
        let stop = PumpStop::new();
        let mut frames = Vec::new();
        while let Some(frame) = source.next_frame(&stop).unwrap() {
            frames.push(frame);
        }
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|frame| frame.data.len() == 480));
        assert_eq!(frames[1].timestamp_ns - frames[0].timestamp_ns, 10_000_000);
    }
}
