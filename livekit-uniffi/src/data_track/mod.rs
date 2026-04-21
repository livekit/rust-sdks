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

use bytes::Bytes;
use livekit_datatrack::api::{DataTrackFrame, DataTrackSid};

pub mod local;
pub mod remote;
pub mod e2ee;

uniffi::custom_type!(DataTrackSid, String, {
    remote,
    lower: |s| String::from(s),
    try_lift: |s| DataTrackSid::try_from(s).map_err(|e| uniffi::deps::anyhow::anyhow!("{e}")),
});

#[uniffi::remote(Record)]
pub struct DataTrackFrame {
    payload: Bytes,
    user_timestamp: Option<u64>
}
/// Information about a published data track.
#[derive(uniffi::Record)]
pub struct DataTrackInfo {
    pub sid: DataTrackSid,
    pub name: String,
    pub uses_e2ee: bool,
}

impl From<&livekit_datatrack::api::DataTrackInfo> for DataTrackInfo {
    fn from(info: &livekit_datatrack::api::DataTrackInfo) -> Self {
        Self { sid: info.sid(), name: info.name().to_string(), uses_e2ee: info.uses_e2ee() }
    }
}

/// Signal response could not be handled.
#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum DataTrackSignalResponseError {
    #[error("Response decoding failed: {0}")]
    Decode(prost::DecodeError),
    #[error("Response container has no message")]
    EmptyMessage,
    #[error("Unsupported response type in this context")]
    UnsupportedType,
    #[error(transparent)]
    Internal(livekit_datatrack::api::InternalError)
}