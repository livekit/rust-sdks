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

use thiserror::Error;
use libwebrtc::native::create_random_uuid;

mod info;
mod incoming;
// mod outgoing;

pub use info::{ByteStreamInfo, TextStreamInfo};
pub use incoming::{ByteStreamReader, TextStreamReader, StreamReader};
pub(crate) use incoming::{IncomingStreamManager, StreamHandlerFuture};

/// Unique identifier of a data stream.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct StreamId(String);

impl StreamId {
    pub(crate) fn new() -> Self {
        Self(create_random_uuid())
    }
}

/// Progress of an incoming or outgoing data stream.
#[derive(Clone, Copy, Default, Debug, Hash, Eq, PartialEq)]
pub struct StreamProgress {
    chunk_index: u64,
    bytes_processed: u64,
    bytes_total: Option<u64>
}

impl StreamProgress {
    fn percentage(&self) -> Option<f32> {
        self.bytes_total
            .map(|total| self.bytes_processed as f32 / total as f32)
    }
}

/// Result type for data stream operations.
pub type StreamResult<T> = Result<T, StreamError>;

/// Error type for data stream operations.
#[derive(Debug, Error)]
pub enum StreamError {
    #[error("stream with this ID is already opened")]
    AlreadyOpened,

    #[error("stream closed abnormally: {0}")]
    AbnormalEnd(String),

    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("incoming header was invalid")]
    InvalidHeader,

    #[error("expected chunk index to be exactly one more than the previous")]
    MissedChunk,

    #[error("read length exceeded total length specified in stream header")]
    LengthExceeded,

    #[error("stream data is incomplete")]
    Incomplete,

    #[error("stream terminated before completion")]
    Terminated,

    #[error("cannot perform operations on unknown stream")]
    UnknownStream,

    #[error("handler already registered for this stream type")]
    HandlerAlreadyRegistered,

    #[error("no handler registered for the incoming stream")]
    HandlerNotRegistered,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error")]
    Internal
}