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

//! Re-exports of the [`livekit_rpc`] crate at the paths RPC has always occupied.
//!
//! This module exists purely for source compatibility — it has no callers inside this crate.
//! Code within `livekit` imports from `livekit_rpc` directly; the production transport lives
//! in [`super::rpc_transport`].

pub use livekit_rpc::api::*;

// Historically public at `livekit::rpc::*`, but not usable without a transport, which is
// internal. Kept reachable so existing code compiles, hidden from the docs.
#[doc(hidden)]
pub use livekit_rpc::backend::{HandleRequestOptions, RpcClientManager, RpcServerManager};
