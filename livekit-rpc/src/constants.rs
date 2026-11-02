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

//! Wire constants shared by the client and server halves.

// RPC protocol version constants (distinct from client_protocol; this is the
// version field on RpcRequest / v2 stream attributes).
pub(crate) const RPC_VERSION_V1: u32 = 1;
pub(crate) const RPC_VERSION_V2: u32 = 2;

/// Data stream topic carrying RPC v2 requests.
pub const RPC_REQUEST_TOPIC: &str = "lk.rpc_request";

/// Data stream topic carrying RPC v2 success responses.
pub const RPC_RESPONSE_TOPIC: &str = "lk.rpc_response";

// Stream attribute keys for RPC v2
pub(crate) const ATTR_REQUEST_ID: &str = "lk.rpc_request_id";
pub(crate) const ATTR_METHOD: &str = "lk.rpc_request_method";
pub(crate) const ATTR_RESPONSE_TIMEOUT_MS: &str = "lk.rpc_request_response_timeout_ms";
pub(crate) const ATTR_VERSION: &str = "lk.rpc_request_version";
