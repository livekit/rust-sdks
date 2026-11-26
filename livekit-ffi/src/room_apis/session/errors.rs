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

//! Exact public core error causes. Display text is context, never a classification input.
use super::NativeSessionError;
use livekit::{rtc_engine::EngineError, RoomError};
use livekit_signaling::SignalError;

pub use livekit_protocol::request_response::Reason as NativeRequestFailureCode;
/// Authoritative request-response reason returned by the server.
#[uniffi::remote(Enum)]
pub enum NativeRequestFailureCode {
    Ok,
    NotFound,
    NotAllowed,
    LimitExceeded,
    Queued,
    UnsupportedType,
    UnclassifiedError,
    InvalidHandle,
    InvalidName,
    DuplicateHandle,
    DuplicateName,
    InvalidRequest,
}
pub use livekit::webrtc::RtcErrorType as NativeRtcFailureKind;
/// Exact WebRTC error category exposed by core.
#[uniffi::remote(Enum)]
pub enum NativeRtcFailureKind {
    Internal,
    InvalidSdp,
    InvalidState,
}

/// Distinguishes authoritative validation status from a WebSocket-upgrade HTTP status.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum NativeHttpFailureSource {
    ClientValidation,
    ServerValidation,
    WebSocketHandshake,
}
/// Structured signaling cause. Transport/region strings have already lost their underlying OS/TLS errors in core.
// Remote Enum requires a Lift/Lower converter for every payload. SignalError
// includes http::StatusCode and prost::DecodeError, neither exported by this
// boundary. Preserve HTTP codes as u16, source variants as an explicit tag, and
// Leave reason/action integers exactly. DecodeError becomes Display text (its
// structured source chain is lost); future unknown variants likewise retain only
// Display text. This is a deliberate error-wire adapter, not mobile classification.
// NativeSessionError additionally represents cursor overflow/cancellation and
// lifecycle failures absent from RoomError; RTC kind/request codes remain core
// enums, and Cow<str> engine context becomes an owned String without text loss.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum NativeSignalFailure {
    UrlParse {
        reason: String,
    },
    TokenFormat,
    Http {
        source: NativeHttpFailureSource,
        status: u16,
        reason: Option<String>,
    },
    Timeout {
        reason: String,
    },
    SendFailed,
    Transport {
        reason: String,
    },
    Closed,
    InvalidState {
        reason: String,
    },
    TransportNotConfigured,
    Region {
        reason: String,
    },
    /// Values are exact livekit protocol DisconnectReason / LeaveRequest.Action integers.
    Leave {
        disconnect_reason: i32,
        action: i32,
    },
    ProtoDecode {
        reason: String,
    },
    Unknown {
        reason: String,
    },
}
impl From<SignalError> for NativeSessionError {
    fn from(error: SignalError) -> Self {
        use NativeHttpFailureSource as H;
        use NativeSignalFailure as F;
        let cause = match error {
            SignalError::Cancelled => return Self::Cancelled,
            SignalError::UrlParse(reason) => F::UrlParse { reason },
            SignalError::TokenFormat => F::TokenFormat,
            SignalError::Client(status, reason) => F::Http {
                source: H::ClientValidation,
                status: status.as_u16(),
                reason: Some(reason),
            },
            SignalError::Server(status, reason) => F::Http {
                source: H::ServerValidation,
                status: status.as_u16(),
                reason: Some(reason),
            },
            SignalError::Handshake { status } => {
                F::Http { source: H::WebSocketHandshake, status: status.as_u16(), reason: None }
            }
            SignalError::Timeout(reason) => F::Timeout { reason },
            SignalError::SendError => F::SendFailed,
            SignalError::Connection(reason) => F::Transport { reason },
            SignalError::Closed => F::Closed,
            SignalError::InvalidState(reason) => F::InvalidState { reason },
            SignalError::TransportNotConfigured => F::TransportNotConfigured,
            SignalError::RegionError(reason) => F::Region { reason },
            SignalError::LeaveRequest { reason, action } => {
                F::Leave { disconnect_reason: reason as i32, action: action as i32 }
            }
            SignalError::ProtoParse(error) => F::ProtoDecode { reason: error.to_string() },
            other => F::Unknown { reason: other.to_string() },
        };
        Self::Signal { failure: cause }
    }
}
impl From<RoomError> for NativeSessionError {
    fn from(error: RoomError) -> Self {
        match error {
            RoomError::Engine(EngineError::Signal(error)) => error.into(),
            RoomError::Engine(EngineError::Rtc(error)) | RoomError::Rtc(error) => {
                Self::Rtc { kind: error.error_type, reason: error.message }
            }
            RoomError::Engine(EngineError::Connection(reason)) => {
                Self::Connection { reason: reason.into_owned() }
            }
            RoomError::Engine(EngineError::Internal(reason)) => {
                Self::Internal { reason: reason.into_owned() }
            }
            RoomError::AlreadyClosed => Self::Closed,
            RoomError::TrackAlreadyPublished => Self::TrackAlreadyPublished,
            RoomError::Request { reason, message } => {
                Self::RequestFailure { code: reason, reason: message }
            }
            RoomError::Internal(reason) => Self::Internal { reason },
        }
    }
}
