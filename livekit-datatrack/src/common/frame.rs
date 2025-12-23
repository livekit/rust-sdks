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

/// Frame published on a data track containing metadata and a payload.
///
/// Construct using [`DataTrackFrameBuilder`].
///
#[derive(Debug)]
pub struct DataTrackFrame {
    pub(crate) payload: Bytes,
    pub(crate) user_timestamp: Option<u64>,
}

impl DataTrackFrame {
    /// Get the frame's payload.
    pub fn payload(&self) -> Bytes {
        self.payload.clone() // Cheap clone
    }

    /// Get the frame's user timestamp, if attached.
    pub fn user_timestamp(&self) -> Option<u64> {
        self.user_timestamp
    }
}

/// Constructs a [`DataTrackFrame`].
#[derive(Default)]
pub struct DataTrackFrameBuilder {
    payload: Bytes,
    user_timestamp: Option<u64>,
}

impl DataTrackFrameBuilder {
    pub fn new(payload: impl Into<Bytes>) -> Self {
        Self { payload: payload.into(), ..Default::default() }
    }

    pub fn user_timestamp(mut self, user_timestamp: u64) -> Self {
        self.user_timestamp = Some(user_timestamp);
        self
    }

    pub fn build(self) -> DataTrackFrame {
        DataTrackFrame { payload: self.payload, user_timestamp: self.user_timestamp }
    }
}

// TODO: just show payload length in debug.
