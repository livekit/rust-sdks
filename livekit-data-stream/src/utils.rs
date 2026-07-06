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

use thiserror::Error;

/// Error returned by the packet transport when a data-stream packet fails to send.
///
/// The stream managers only need to know that a send failed (they map it to
/// [`StreamError::SendFailed`]); the concrete engine error type stays in the `livekit` crate,
/// which bridges the outgoing packet channel to the RTC engine.
#[derive(Debug, Clone)]
pub struct SendError;

/// Result type for data stream operations.
pub type StreamResult<T> = Result<T, StreamError>;

/// Error type for data stream operations.
#[derive(Debug, Error)]
pub enum StreamError {
    // TODO(ladvoc): standardize error cases and expose over FFI.
    #[error("stream has already been closed")]
    AlreadyClosed,

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

    #[error("unable to send packet")]
    SendFailed,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error")]
    Internal,

    #[error("encryption type mismatch")]
    EncryptionTypeMismatch,

    #[error("stream header exceeds maximum size")]
    HeaderTooLarge,

    #[error("decompression failed")]
    Decompression,
}

/// Progress of a data stream.
#[derive(Clone, Copy, Default, Debug, Hash, Eq, PartialEq)]
pub(crate) struct StreamProgress {
    pub(crate) chunk_index: u64,
    /// Number of bytes read or written so far.
    pub(crate) bytes_processed: u64,
    /// Total number of bytes expected to be read or written for finite streams.
    pub(crate) bytes_total: Option<u64>,
}

impl StreamProgress {
    /// Returns the completion percentage for finite streams.
    #[allow(dead_code)]
    fn percentage(&self) -> Option<f32> {
        self.bytes_total.map(|total| self.bytes_processed as f32 / total as f32)
    }
}
