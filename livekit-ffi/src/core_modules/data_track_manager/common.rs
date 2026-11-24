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

use livekit_protocol as proto;
use prost::Message;

/// Signal response crossing the FFI boundary could not be processed.
#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum HandleSignalResponseError {
    #[error("Response decoding failed: {0}")]
    Decode(prost::DecodeError),
    #[error("Response container has no message")]
    EmptyMessage,
    #[error("Unsupported response type in this context")]
    UnsupportedType,
    #[error(transparent)]
    Internal(livekit_datatrack::api::InternalError),
}

/// Deserializes a signal response crossing the FFI boundary, returning the message variant.
pub(crate) fn deserialize_signal_response(
    res: &[u8],
) -> Result<proto::signal_response::Message, HandleSignalResponseError> {
    let res = proto::SignalResponse::decode(res).map_err(HandleSignalResponseError::Decode)?;
    res.message.ok_or(HandleSignalResponseError::EmptyMessage)
}
