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

//! Pumps encoded access units from a capture source into an RTC video
//! source.

use crate::{
    encoded::{
        CodecSpecific, EncodedFrameType, EncodedLayerInfo, EncodedVideoSource,
        OwnedEncodedAccessUnit, RateControl,
    },
    error::CaptureError,
    pump::{spawn_pump, PumpError, PumpExit, PumpStats, PumpStop, RunningPump},
};
use livekit::{
    options::{TrackPublishOptions, VideoEncoderBackend},
    webrtc::{
        video_frame::EncodedVideoFrame,
        video_source::{native::NativeVideoSource, EncodedRateControl, RtcVideoSource},
    },
};
use std::{fmt, io};

impl From<EncodedRateControl> for RateControl {
    fn from(target: EncodedRateControl) -> Self {
        Self { target_bitrate_bps: target.target_bitrate_bps, framerate_fps: target.framerate_fps }
    }
}

/// Pumps an [`EncodedVideoSource`] into an RTC video source, publishing
/// access units as passthrough.
///
/// Downstream keyframe requests (PLI/FIR, late subscriber) and rate-control
/// targets are polled between access units and forwarded to the source.
/// Pre-roll delta frames are dropped until the first keyframe, since
/// decoding can only start at a keyframe.
pub struct EncodedVideoPump<S: EncodedVideoSource> {
    source: S,
    rtc_source: NativeVideoSource,
    stop: PumpStop,
}

impl<S: EncodedVideoSource> EncodedVideoPump<S> {
    /// Creates a pump for an encoded source, building the matching RTC
    /// source.
    pub fn new(source: S) -> Self {
        let rtc_source = NativeVideoSource::new_encoded(source.resolution().into());
        Self { source, rtc_source, stop: PumpStop::new() }
    }

    /// Returns the RTC source to create the local track with.
    pub fn rtc_source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.rtc_source.clone())
    }

    /// Returns publish options for encoded passthrough.
    pub fn publish_options(&self) -> TrackPublishOptions {
        TrackPublishOptions {
            video_codec: self.source.codec().into(),
            video_encoder: VideoEncoderBackend::PreEncoded,
            simulcast: false,
            ..Default::default()
        }
    }

    /// Returns a cancellation handle for this pump.
    pub fn stop_handle(&self) -> PumpStop {
        self.stop.clone()
    }

    /// Returns the underlying capture source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Runs the pump on the calling thread until the source ends, a failure,
    /// or the stop handle fires.
    ///
    /// Sources block, so callers on an async runtime should run this on a
    /// dedicated thread (see [`EncodedVideoPump::spawn`]) or a blocking pool.
    pub fn run(mut self) -> Result<PumpStats, PumpError> {
        let mut frames_captured = 0;
        let mut awaiting_initial_keyframe = true;
        let exit = loop {
            if self.stop.is_stopped() {
                break PumpExit::Stopped;
            }
            if let Some(target) = self.rtc_source.take_rate_control_request() {
                self.source.update_rate_control(target.into());
            }
            if self.rtc_source.take_keyframe_request() {
                self.source.request_keyframe();
            }

            let Some(access_unit) = self.source.next_access_unit()? else {
                break PumpExit::EndOfStream;
            };

            // Drop pre-roll deltas: decoding can only start at a keyframe.
            if awaiting_initial_keyframe && access_unit.frame_type != EncodedFrameType::Key {
                continue;
            }
            awaiting_initial_keyframe = false;

            capture_access_unit(&self.rtc_source, &access_unit)?;
            frames_captured += 1;
        };
        Ok(PumpStats { frames_captured, exit })
    }

    /// Runs the pump on a dedicated thread.
    ///
    /// Panics on the pump thread are caught and reported as
    /// [`PumpError::Panicked`] when the pump is joined.
    pub fn spawn(self) -> io::Result<RunningPump>
    where
        S: 'static,
    {
        let stop = self.stop_handle();
        spawn_pump(stop, move || self.run())
    }
}

impl<S: EncodedVideoSource> fmt::Debug for EncodedVideoPump<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedVideoPump")
            .field("rtc_source", &self.rtc_source)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

fn capture_access_unit(
    rtc_source: &NativeVideoSource,
    access_unit: &OwnedEncodedAccessUnit,
) -> Result<(), CaptureError> {
    validate_access_unit(access_unit)?;

    let frame = EncodedVideoFrame {
        codec: access_unit.codec.into(),
        payload: &access_unit.payload,
        timestamp_us: access_unit.timestamp_us,
        frame_type: access_unit.frame_type.into(),
        resolution: access_unit.resolution.into(),
        frame_metadata: None,
    };
    rtc_source.capture_encoded_frame(&frame).then_some(()).ok_or(CaptureError::CaptureFailed)
}

/// The passthrough path forwards single-layer streams: access units carrying
/// temporal/spatial layer ids or layering metadata are rejected so callers
/// are not misled into thinking that metadata reaches the wire.
fn validate_access_unit(access_unit: &OwnedEncodedAccessUnit) -> Result<(), CaptureError> {
    if access_unit.payload.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    if access_unit.layers != EncodedLayerInfo::default() {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "temporal/spatial layer ids are not forwarded by the passthrough encoder",
        ));
    }
    if access_unit.codec_specific != CodecSpecific::None
        && access_unit.codec_specific != CodecSpecific::default_for(access_unit.codec)
    {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "codec-specific layering metadata is not forwarded by the passthrough encoder",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use bytes::Bytes;

    use super::*;
    use crate::{encoded::EncodedVideoCodec, error::SourceError, primitive::VideoResolution};

    const RESOLUTION: VideoResolution = VideoResolution { width: 64, height: 36 };

    struct FakeEncodedSource {
        access_units: VecDeque<OwnedEncodedAccessUnit>,
    }

    impl FakeEncodedSource {
        fn new(access_units: impl IntoIterator<Item = OwnedEncodedAccessUnit>) -> Self {
            Self { access_units: access_units.into_iter().collect() }
        }
    }

    impl EncodedVideoSource for FakeEncodedSource {
        fn resolution(&self) -> VideoResolution {
            RESOLUTION
        }

        fn codec(&self) -> EncodedVideoCodec {
            EncodedVideoCodec::VP8
        }

        fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
            Ok(self.access_units.pop_front())
        }
    }

    fn access_unit(timestamp_us: i64, frame_type: EncodedFrameType) -> OwnedEncodedAccessUnit {
        OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::VP8,
            vec![1, 2, 3],
            timestamp_us,
            frame_type,
            RESOLUTION,
        )
    }

    #[test]
    fn encoded_pump_starts_at_initial_keyframe() {
        let source = FakeEncodedSource::new([
            access_unit(1, EncodedFrameType::Delta),
            access_unit(2, EncodedFrameType::Delta),
            access_unit(3, EncodedFrameType::Key),
            access_unit(4, EncodedFrameType::Delta),
        ]);
        let stats = EncodedVideoPump::new(source).run().unwrap();
        assert_eq!(stats.frames_captured, 2);
    }

    #[test]
    fn boxed_source_drives_generic_pump() {
        // The dynamic-instantiation pattern: box at the edge, same pump.
        let source: Box<dyn EncodedVideoSource> =
            Box::new(FakeEncodedSource::new([access_unit(1, EncodedFrameType::Key)]));
        let pump = EncodedVideoPump::new(source);
        assert_eq!(pump.publish_options().video_encoder, VideoEncoderBackend::PreEncoded);
        let stats = pump.run().unwrap();
        assert_eq!(stats.frames_captured, 1);
    }

    #[test]
    fn encoded_pump_rejects_empty_payloads() {
        let mut unit = access_unit(1, EncodedFrameType::Key);
        unit.payload = Bytes::new();

        let result = EncodedVideoPump::new(FakeEncodedSource::new([unit])).run();
        assert!(matches!(result, Err(PumpError::Capture(CaptureError::EmptyPayload))));
    }

    #[test]
    fn encoded_publish_options_use_passthrough() {
        let pump = EncodedVideoPump::new(FakeEncodedSource::new([]));
        let options = pump.publish_options();
        assert_eq!(options.video_encoder, VideoEncoderBackend::PreEncoded);
        assert!(!options.simulcast);
    }
}
