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

use std::{error::Error, fmt};

use crate::{encoded::OwnedEncodedAccessUnit, error::CaptureError, track::VideoCaptureTrack};

/// Source of owned encoded access units.
pub trait EncodedAccessUnitSource {
    /// Error returned by the source.
    type Error: Error + Send + Sync + 'static;

    /// Returns the next encoded access unit, or `Ok(None)` when the source reaches EOF.
    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error>;
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

/// Pulls encoded access units from a source and forwards them into a video track.
#[derive(Debug)]
pub struct EncodedIngress<S> {
    track: VideoCaptureTrack,
    source: S,
}

impl<S> EncodedIngress<S> {
    /// Creates an encoded ingress runner.
    pub fn new(track: VideoCaptureTrack, source: S) -> Self {
        Self { track, source }
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

impl<S> EncodedIngress<S>
where
    S: EncodedAccessUnitSource,
{
    /// Captures the next access unit and returns `false` after source EOF.
    pub fn capture_next(&mut self) -> Result<bool, EncodedIngressError<S::Error>> {
        let Some(access_unit) =
            self.source.next_access_unit().map_err(EncodedIngressError::Source)?
        else {
            return Ok(false);
        };

        self.track
            .capture_encoded(&access_unit.as_access_unit())
            .map_err(EncodedIngressError::Capture)?;
        Ok(true)
    }

    /// Captures access units until the source reaches EOF.
    pub fn run_until_end(&mut self) -> Result<u64, EncodedIngressError<S::Error>> {
        let mut captured = 0;
        while self.capture_next()? {
            captured += 1;
        }
        Ok(captured)
    }
}
