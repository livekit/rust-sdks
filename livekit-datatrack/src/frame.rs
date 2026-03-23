// Copyright 2025 LiveKit, Inc.
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

use bytes::Bytes;
use core::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A frame published on a data track, consisting of a payload and optional metadata.
///
/// # Examples
///
/// Create a frame from a [`Vec`] payload:
///
/// ```
/// # use livekit_datatrack::api::DataTrackFrame;
/// let some_payload = vec![0xFA; 256];
/// let frame: DataTrackFrame = some_payload.into();
///
/// assert_eq!(frame.payload().len(), 256);
/// ```
///
#[derive(Clone, Default)]
pub struct DataTrackFrame {
    pub(crate) payload: Bytes,
    pub(crate) user_timestamp: Option<u64>,
}

impl DataTrackFrame {
    /// Returns the frame's payload.
    pub fn payload(&self) -> Bytes {
        self.payload.clone() // Cheap clone
    }

    /// Returns the frame's user timestamp, if one is associated.
    pub fn user_timestamp(&self) -> Option<u64> {
        self.user_timestamp
    }

    /// If the frame has a user timestamp, calculate how long has passed
    /// relative to the current system time.
    ///
    /// If a timestamp is present, it is assumed it is a UNIX timestamp in milliseconds
    /// (as can be set with [`Self::with_user_timestamp_now`] on the publisher side).
    /// If the timestamp is invalid or not present, the result is none.
    ///
    pub fn duration_since_timestamp(&self) -> Option<Duration> {
        let ts = self.user_timestamp?;
        let ts_time = UNIX_EPOCH.checked_add(Duration::from_millis(ts))?;
        SystemTime::now()
            .duration_since(ts_time)
            .inspect_err(|err| log::error!("Failed to calculate duration: {err}"))
            .ok()
    }
}

impl DataTrackFrame {
    /// Creates a frame from the given payload.
    pub fn new(payload: impl Into<Bytes>) -> Self {
        Self { payload: payload.into(), ..Default::default() }
    }

    /// Associates a user timestamp with the frame.
    pub fn with_user_timestamp(mut self, value: u64) -> Self {
        self.user_timestamp = Some(value);
        self
    }

    /// Associates the current Unix timestamp (in milliseconds) with the frame.
    pub fn with_user_timestamp_now(mut self) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .inspect_err(|err| log::error!("Failed to get system time: {err}"))
            .ok();
        self.user_timestamp = timestamp;
        self
    }
}

impl fmt::Debug for DataTrackFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataTrackFrame")
            .field("payload_len", &self.payload.len())
            .field("user_timestamp", &self.user_timestamp)
            .finish()
    }
}

// MARK: - From implementations

impl From<Bytes> for DataTrackFrame {
    fn from(bytes: Bytes) -> Self {
        Self { payload: bytes, ..Default::default() }
    }
}

impl From<&'static [u8]> for DataTrackFrame {
    fn from(slice: &'static [u8]) -> Self {
        Self { payload: slice.into(), ..Default::default() }
    }
}

impl From<Vec<u8>> for DataTrackFrame {
    fn from(vec: Vec<u8>) -> Self {
        Self { payload: vec.into(), ..Default::default() }
    }
}

impl From<Box<[u8]>> for DataTrackFrame {
    fn from(slice: Box<[u8]>) -> Self {
        Self { payload: slice.into(), ..Default::default() }
    }
}
