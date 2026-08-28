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

//! Pure domain types for RPC: call parameters, invocation data, and errors.

use livekit_common::ParticipantIdentity;
use livekit_protocol::RpcError as RpcError_Proto;
use std::{error::Error, fmt::Display, time::Duration};

/// Parameters for performing an RPC call
#[derive(Debug, Clone)]
pub struct PerformRpcData {
    pub destination_identity: String,
    pub method: String,
    pub payload: String,
    pub response_timeout: Duration,
    pub max_round_trip_latency: Duration,
}

impl PerformRpcData {
    pub fn new(
        destination_identity: impl Into<ParticipantIdentity>,
        method: impl Into<String>,
    ) -> Self {
        Self::default().with_destination_identity(destination_identity).with_method(method)
    }

    pub fn with_destination_identity(
        mut self,
        destination_identity: impl Into<ParticipantIdentity>,
    ) -> Self {
        let identity: ParticipantIdentity = destination_identity.into();
        self.destination_identity = identity.into();
        self
    }
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = method.into();
        self
    }
    pub fn with_payload(mut self, payload: impl Into<String>) -> Self {
        self.payload = payload.into();
        self
    }

    /// The maximum time the caller will wait for a response after the request has been
    /// acknowledged by the remote. Defaults to 15 seconds.
    pub fn with_response_timeout(mut self, response_timeout: Duration) -> Self {
        self.response_timeout = response_timeout;
        self
    }

    /// Maximum amount of time it should ever take for an RPC request to reach the
    /// destination and for the ACK to come back. Most callers should not need
    /// to change this, but it can be increased to tolerate high-latency networks where
    /// RPC requests are backed up behind other messages on the data channel.
    /// Defaults to 7 seconds.
    pub fn with_max_round_trip_latency(mut self, max_round_trip_latency: Duration) -> Self {
        self.max_round_trip_latency = max_round_trip_latency;
        self
    }
}

impl Default for PerformRpcData {
    fn default() -> Self {
        Self {
            destination_identity: Default::default(),
            method: Default::default(),
            payload: Default::default(),
            response_timeout: Duration::from_secs(15),
            max_round_trip_latency: Duration::from_secs(7),
        }
    }
}

/// Data passed to method handler for incoming RPC invocations
///
/// Attributes:
///     request_id (String): The unique request ID. Will match at both sides of the call, useful for debugging or logging.
///     caller_identity (ParticipantIdentity): The unique participant identity of the caller.
///     payload (String): The payload of the request. User-definable format, typically JSON.
///     response_timeout (Duration): The maximum time the caller will wait for a response.
#[derive(Debug, Clone)]
pub struct RpcInvocationData {
    pub request_id: String,
    pub caller_identity: ParticipantIdentity,
    pub payload: String,
    pub response_timeout: Duration,
}

/// Specialized error handling for RPC methods.
///
/// Instances of this type, when thrown in a method handler, will have their `message`
/// serialized and sent across the wire. The caller will receive an equivalent error on the other side.
///
/// Built-in types are included but developers may use any string, with a max length of 256 bytes.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: u32,
    pub message: String,
    pub data: Option<String>,
}

impl RpcError {
    pub const MAX_MESSAGE_BYTES: usize = 256;
    pub const MAX_DATA_BYTES: usize = MAX_V1_PAYLOAD_BYTES;

    /// Creates an error object with the given code and message, plus an optional data payload.
    ///
    /// If thrown in an RPC method handler, the error will be sent back to the caller.
    ///
    /// Error codes 1001-1999 are reserved for built-in errors (see RpcErrorCode for their meanings).
    pub fn new(code: u32, message: String, data: Option<String>) -> Self {
        Self {
            code,
            message: truncate_bytes(&message, Self::MAX_MESSAGE_BYTES),
            data: data.map(|d| truncate_bytes(&d, Self::MAX_DATA_BYTES)),
        }
    }

    pub fn from_proto(proto: RpcError_Proto) -> Self {
        Self::new(proto.code, proto.message, Some(proto.data))
    }

    pub fn to_proto(&self) -> RpcError_Proto {
        RpcError_Proto {
            code: self.code,
            message: self.message.clone(),
            data: self.data.clone().unwrap_or_default(),
        }
    }
}

impl Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RPC Error: {} ({})", self.message, self.code)
    }
}
impl Error for RpcError {}

#[derive(Debug, Clone, Copy)]
pub enum RpcErrorCode {
    ApplicationError = 1500,
    ConnectionTimeout = 1501,
    ResponseTimeout = 1502,
    RecipientDisconnected = 1503,
    ResponsePayloadTooLarge = 1504,
    SendFailed = 1505,

    UnsupportedMethod = 1400,
    RecipientNotFound = 1401,
    RequestPayloadTooLarge = 1402,
    UnsupportedServer = 1403,
    UnsupportedVersion = 1404,
}

impl RpcErrorCode {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::ApplicationError => "Application error in method handler",
            Self::ConnectionTimeout => "Connection timeout",
            Self::ResponseTimeout => "Response timeout",
            Self::RecipientDisconnected => "Recipient disconnected",
            Self::ResponsePayloadTooLarge => "Response payload too large",
            Self::SendFailed => "Failed to send",

            Self::UnsupportedMethod => "Method not supported at destination",
            Self::RecipientNotFound => "Recipient not found",
            Self::RequestPayloadTooLarge => "Request payload too large",
            Self::UnsupportedServer => "RPC not supported by server",
            Self::UnsupportedVersion => "Unsupported RPC version",
        }
    }
}

impl RpcError {
    /// Creates an error object from the code, with an auto-populated message.
    pub fn built_in(code: RpcErrorCode, data: Option<String>) -> Self {
        Self::new(code as u32, code.message().to_string(), data)
    }
}

/// Maximum payload size in bytes for RPC v1
pub const MAX_V1_PAYLOAD_BYTES: usize = 15360; // 15 KB

/// Calculate the byte length of a string
fn byte_length(s: &str) -> usize {
    s.as_bytes().len()
}

/// Truncate a string to a maximum number of bytes
fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if byte_length(s) <= max_bytes {
        return s.to_string();
    }

    let mut result = String::new();
    for c in s.chars() {
        if byte_length(&(result.clone() + &c.to_string())) > max_bytes {
            break;
        }
        result.push(c);
    }
    result
}
