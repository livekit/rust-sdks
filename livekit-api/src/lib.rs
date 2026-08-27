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

#[cfg(any(feature = "services-tokio", feature = "services-async"))]
pub mod services;

// The signalling client lives in the livekit-signaling crate. Unlike
// `access_token`, this path is NOT supported API: nothing in the workspace uses
// it any more and it exists only so existing dependents keep compiling.
//
// The deprecation fires on `use livekit_api::signal_client;` but not on
// `use livekit_api::signal_client::{Item}` — rustc only lints a deprecated
// module when it is the final path segment. `#[doc(hidden)]` keeps it out of the
// published docs so it stops reading as blessed API.
#[cfg(feature = "signal-client")]
#[deprecated(note = "internal SDK API; depend on livekit-signaling directly")]
#[doc(hidden)]
pub mod signal_client {
    pub use livekit_signaling::*;
}

#[cfg(any(feature = "services-tokio", feature = "services-async"))]
mod http_client;

#[cfg(feature = "webhooks")]
pub mod webhooks;
