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

use crate::dtp::{Dtp, Extensions, FrameMarker};
use bytes::Bytes;

/// Converts packets into application-level frames.
pub struct Depacketizer;

/// Output of [`Depacketizer`].
pub struct DepacketizerFrame {
    pub payload: Bytes,
    pub extensions: Extensions,
}

impl Depacketizer {
    /// Creates a new depacketizer.
    pub fn new() -> Self {
        Self
    }

    /// Push a packet into the depacketizer, returning a complete frame if one is available.
    pub fn push(&mut self, dtp: Dtp) -> Option<DepacketizerFrame> {
        match dtp.header.marker {
            FrameMarker::Single => {
                DepacketizerFrame { payload: dtp.payload, extensions: dtp.header.extensions }.into()
            }
            _ => unimplemented!("Multi-packet frames not supported yet"),
        }
    }
}
