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

use livekit::{RoomError, StreamError};
use livekit_token_source::TokenSourceError;
use thiserror::Error;

/// Why [`Session::start`](crate::Session::start) failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SessionStartError {
    /// The session is already connecting or connected.
    #[error("session is already started")]
    AlreadyStarted,
    /// [`Session::end`](crate::Session::end) was called before the connection
    /// completed.
    #[error("session ended before it connected")]
    Cancelled,
    /// The token source failed to provide credentials.
    #[error("failed to fetch credentials: {0}")]
    Token(#[from] TokenSourceError),
    /// The room failed to connect.
    #[error("failed to connect: {0}")]
    Connect(#[from] RoomError),
}

/// Why [`Session::end`](crate::Session::end) failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SessionEndError {
    /// The session is not started, or has already ended or disconnected.
    #[error("session is not started")]
    NotStarted,
    /// The room failed to close cleanly. The session has ended regardless.
    #[error("failed to close the room: {0}")]
    Room(#[from] RoomError),
}

/// Why [`Session::send_text`](crate::Session::send_text) failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MessageSendError {
    /// The session has no connected room.
    #[error("session is not connected")]
    NotConnected,
    /// The text stream failed to send.
    #[error("failed to send: {0}")]
    Stream(#[from] StreamError),
}
