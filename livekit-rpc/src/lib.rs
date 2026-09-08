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

mod client;
mod constants;
mod server;
mod transport;
mod types;

#[cfg(test)]
mod tests;

/// Public API re-exported by client SDKs (surfaced to end users through the `livekit` crate).
pub mod api {
    pub use crate::types::{
        PerformRpcData, RpcError, RpcErrorCode, RpcInvocationData, MAX_V1_PAYLOAD_BYTES,
    };
}

/// Internal APIs used within the `livekit` SDK to power RPC.
pub mod backend {
    pub use crate::client::RpcClientManager;
    pub use crate::constants::{RPC_REQUEST_TOPIC, RPC_RESPONSE_TOPIC};
    pub use crate::server::{HandleRequestOptions, RpcServerManager};
    pub use crate::transport::{RpcTransport, RpcTransportError};
}
