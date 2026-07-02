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

use crate::{encoded::OwnedEncodedAccessUnit, error::CaptureError, track::VideoCaptureTrack};

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
    track: VideoCaptureTrack,
    source: S,
    stop: EncodedIngressStop,
}

impl<S> EncodedIngress<S> {
    /// Creates an encoded ingress runner.
    pub fn new(track: VideoCaptureTrack, source: S) -> Self {
        Self { track, source, stop: EncodedIngressStop::new() }
    }

    /// Returns a cancellation handle for this runner.
    pub fn stop_handle(&self) -> EncodedIngressStop {
        self.stop.clone()
    }

    /// Returns the capture track used by this runner.
    pub fn track(&self) -> &VideoCaptureTrack {
        &self.track
    }

    /// Returns the underlying encoded source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Returns the underlying encoded source mutably.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// Consumes this runner and returns its parts.
    pub fn into_parts(self) -> (VideoCaptureTrack, S) {
        (self.track, self.source)
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
    /// Downstream keyframe requests (PLI/FIR raised by the passthrough
    /// encoder) are polled on every call and forwarded to the source via
    /// [`EncodedAccessUnitSource::request_keyframe`].
    pub fn capture_next(
        &mut self,
    ) -> Result<Option<EncodedIngressCapture>, EncodedIngressError<S::Error>> {
        if self.track.take_keyframe_request() {
            self.source.request_keyframe();
        }

        let Some(access_unit) =
            self.source.next_access_unit().map_err(EncodedIngressError::Source)?
        else {
            return Ok(None);
        };

        self.track
            .capture_encoded(&access_unit.as_access_unit())
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
