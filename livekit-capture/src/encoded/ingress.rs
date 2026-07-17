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

use std::{
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use livekit::webrtc::{video_frame::FrameMetadata, video_source::native::NativeVideoSource};

use crate::{
    encoded::{EncodedFrameType, EncodedRateControl, OwnedEncodedAccessUnit},
    error::CaptureError,
    track::NativeVideoSourceExt,
};

/// Source of owned encoded access units.
pub trait EncodedAccessUnitSource {
    /// Error returned by the source.
    type Error: Error + Send + Sync + 'static;

    /// Returns the next encoded access unit, or `Ok(None)` when the source reaches EOF.
    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error>;

    /// Forwards a downstream keyframe request (PLI/FIR, late subscriber) to
    /// the producer so it can emit an IDR.
    ///
    /// The default implementation does nothing, for transports that cannot
    /// influence the upstream encoder.
    fn request_keyframe(&mut self) {}

    /// Forwards a downstream rate-control target to the producer.
    ///
    /// The default implementation does nothing, for transports that cannot
    /// influence the upstream encoder.
    fn update_rate_control(&mut self, _rate_control: EncodedRateControl) {}
}

/// Error returned while forwarding encoded access units into a track.
#[derive(Debug)]
pub enum EncodedIngressError<E> {
    /// The encoded source failed.
    Source(E),
    /// The capture track rejected an access unit.
    Capture(CaptureError),
}

impl<E: fmt::Display> fmt::Display for EncodedIngressError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(err) => write!(f, "encoded source failed: {err}"),
            Self::Capture(err) => write!(f, "encoded capture failed: {err}"),
        }
    }
}

impl<E> Error for EncodedIngressError<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(err) => Some(err),
            Self::Capture(err) => Some(err),
        }
    }
}

/// Cancellation handle for [`EncodedIngress::run_until_end`].
///
/// Cheap to clone; wire it to a shutdown signal (e.g. Ctrl-C) and call
/// [`EncodedIngressStop::stop`] from any thread to make the ingest loop
/// return after the access unit in flight.
#[derive(Debug, Clone, Default)]
pub struct EncodedIngressStop(Arc<AtomicBool>);

impl EncodedIngressStop {
    /// Creates an un-stopped handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals the ingest loop to stop.
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns true once [`EncodedIngressStop::stop`] has been called.
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Pulls encoded access units from a source and forwards them into a video track.
#[derive(Debug)]
pub struct EncodedIngress<S> {
    rtc_source: NativeVideoSource,
    capture_source: S,
    stop: EncodedIngressStop,
    awaiting_initial_keyframe: bool,
}

impl<S> EncodedIngress<S> {
    /// Creates an encoded ingress runner.
    pub fn new(rtc_source: NativeVideoSource, capture_source: S) -> Self {
        Self {
            rtc_source,
            capture_source,
            stop: EncodedIngressStop::new(),
            awaiting_initial_keyframe: true,
        }
    }

    /// Returns a cancellation handle for this runner.
    pub fn stop_handle(&self) -> EncodedIngressStop {
        self.stop.clone()
    }

    /// Returns the RTC source used by this runner.
    pub fn rtc_source(&self) -> &NativeVideoSource {
        &self.rtc_source
    }

    /// Returns the underlying encoded source.
    pub fn source(&self) -> &S {
        &self.capture_source
    }

    /// Returns the underlying encoded source mutably.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.capture_source
    }

    /// Consumes this runner and returns its parts.
    pub fn into_parts(self) -> (NativeVideoSource, S) {
        (self.rtc_source, self.capture_source)
    }
}

/// Details of one access unit captured by [`EncodedIngress::capture_next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedIngressCapture {
    /// Capture timestamp of the access unit in microseconds.
    pub timestamp_us: i64,
    /// Frame type of the access unit.
    pub frame_type: crate::encoded::EncodedFrameType,
    /// Payload size in bytes.
    pub payload_len: usize,
}

impl<S> EncodedIngress<S>
where
    S: EncodedAccessUnitSource,
{
    /// Captures the next access unit, returning `None` after source EOF.
    ///
    /// Downstream rate-control and keyframe requests raised by the
    /// passthrough encoder are polled on every call and forwarded to the
    /// source via [`EncodedAccessUnitSource::update_rate_control`] and
    /// [`EncodedAccessUnitSource::request_keyframe`].
    pub fn capture_next(
        &mut self,
    ) -> Result<Option<EncodedIngressCapture>, EncodedIngressError<S::Error>> {
        self.capture_next_with_metadata(|_| None)
    }

    /// Captures the next access unit with metadata generated after the source yields it.
    ///
    /// The metadata producer is not called for skipped pre-roll frames while
    /// the ingress runner is waiting for the initial keyframe.
    pub fn capture_next_with_metadata(
        &mut self,
        frame_metadata: impl FnOnce(&OwnedEncodedAccessUnit) -> Option<FrameMetadata>,
    ) -> Result<Option<EncodedIngressCapture>, EncodedIngressError<S::Error>> {
        if let Some(rate_control) = self.rtc_source.take_rate_control_request() {
            self.capture_source.update_rate_control(rate_control);
        }
        if self.rtc_source.take_keyframe_request() {
            self.capture_source.request_keyframe();
        }

        let access_unit = loop {
            let Some(access_unit) =
                self.capture_source.next_access_unit().map_err(EncodedIngressError::Source)?
            else {
                return Ok(None);
            };

            if !self.awaiting_initial_keyframe || access_unit.frame_type == EncodedFrameType::Key {
                self.awaiting_initial_keyframe = false;
                break access_unit;
            }
        };

        let frame_metadata = frame_metadata(&access_unit);
        self.rtc_source
            .capture_encoded_with_metadata(&access_unit.as_access_unit(), frame_metadata)
            .map_err(EncodedIngressError::Capture)?;
        Ok(Some(EncodedIngressCapture {
            timestamp_us: access_unit.timestamp_us,
            frame_type: access_unit.frame_type,
            payload_len: access_unit.payload.len(),
        }))
    }

    /// Captures access units until the source reaches EOF or the stop
    /// handle fires, returning the number of captured access units.
    pub fn run_until_end(&mut self) -> Result<u64, EncodedIngressError<S::Error>> {
        let mut captured = 0;
        while !self.stop.is_stopped() && self.capture_next()?.is_some() {
            captured += 1;
        }
        Ok(captured)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, error::Error, fmt};

    use super::*;
    use crate::{encoded::EncodedVideoCodec, primitives::VideoResolution};

    #[derive(Debug)]
    struct FakeSourceError;

    impl fmt::Display for FakeSourceError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("fake source failed")
        }
    }

    impl Error for FakeSourceError {}

    #[derive(Debug)]
    struct FakeSource {
        access_units: VecDeque<OwnedEncodedAccessUnit>,
    }

    impl FakeSource {
        fn new(access_units: impl IntoIterator<Item = OwnedEncodedAccessUnit>) -> Self {
            Self { access_units: access_units.into_iter().collect() }
        }
    }

    impl EncodedAccessUnitSource for FakeSource {
        type Error = FakeSourceError;

        fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
            Ok(self.access_units.pop_front())
        }
    }

    fn access_unit(timestamp_us: i64, frame_type: EncodedFrameType) -> OwnedEncodedAccessUnit {
        OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::VP8,
            vec![1, 2, 3],
            timestamp_us,
            frame_type,
            VideoResolution::new(640, 480),
        )
    }

    fn rtc_source() -> NativeVideoSource {
        NativeVideoSource::new_encoded(VideoResolution::new(640, 480).into())
    }

    #[test]
    fn capture_next_starts_at_initial_keyframe() {
        let source = FakeSource::new([
            access_unit(1, EncodedFrameType::Delta),
            access_unit(2, EncodedFrameType::Delta),
            access_unit(3, EncodedFrameType::Key),
        ]);
        let mut ingress = EncodedIngress::new(rtc_source(), source);

        let capture = ingress
            .capture_next()
            .expect("capture should succeed")
            .expect("keyframe should be captured");

        assert_eq!(capture.timestamp_us, 3);
        assert_eq!(capture.frame_type, EncodedFrameType::Key);
    }

    #[test]
    fn capture_next_allows_deltas_after_initial_keyframe() {
        let source = FakeSource::new([
            access_unit(1, EncodedFrameType::Key),
            access_unit(2, EncodedFrameType::Delta),
        ]);
        let mut ingress = EncodedIngress::new(rtc_source(), source);

        let first = ingress.capture_next().unwrap().unwrap();
        let second = ingress.capture_next().unwrap().unwrap();

        assert_eq!(first.frame_type, EncodedFrameType::Key);
        assert_eq!(second.frame_type, EncodedFrameType::Delta);
        assert_eq!(second.timestamp_us, 2);
    }
}
