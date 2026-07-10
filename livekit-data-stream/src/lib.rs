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

#![doc = include_str!("../README.md")]

mod incoming;
mod info;
mod outgoing;
#[cfg(any(test, feature = "test-utils"))]
mod test_utils;
mod types;
mod utf8_chunk;
mod utils;

/// Public API re-exported by client SDKs (surfaced to end users through the `livekit` crate).
pub mod api {
    pub use crate::incoming::{AnyStreamReader, ByteStreamReader, StreamReader, TextStreamReader};
    pub use crate::info::{ByteStreamInfo, TextStreamInfo};
    pub use crate::outgoing::{
        ByteStreamWriter, StreamByteOptions, StreamTextOptions, StreamWriter, TextStreamWriter,
    };
    pub use crate::types::OperationType;
    pub use crate::utils::{SendError, StreamError, StreamResult};
}

/// Internal APIs used within the `livekit` SDK to power data streams.
pub mod backend {
    // Wire types + their proto conversions, used by the room to build packets and events.
    pub use crate::types::{
        ByteHeader, Chunk, CompressionType, ContentHeader, Header, OperationType, Packet, StreamId,
        TextHeader, Trailer,
    };

    /// Incoming data streams.
    pub mod incoming {
        pub use crate::incoming::{events::*, manager::*};
    }

    /// Outgoing data streams.
    pub mod outgoing {
        pub use crate::outgoing::manager::*;
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub use crate::test_utils::*;
}
