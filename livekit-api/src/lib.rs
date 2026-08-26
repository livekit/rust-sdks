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

#![doc = include_str!("../README.md")]

// The token implementation lives in the livekit-token crate. This alias keeps
// the historical `livekit_api::access_token::*` paths working, and remains the
// documented, supported way to reach these types.
#[cfg(feature = "access-token")]
#[doc(inline)]
pub use livekit_token as access_token;

#[cfg(feature = "__services")]
pub mod services;

#[cfg(feature = "signal-client")]
pub mod signal_client;

#[cfg(feature = "__services")]
mod http_client;

// Region-discovery helpers shared by the signaling region provider
// (signal_client::region_url_provider) and the API failover region cache
// (services::failover).
#[cfg(any(feature = "signal-client", feature = "__services"))]
mod region;

#[cfg(feature = "webhooks")]
pub mod webhooks;
