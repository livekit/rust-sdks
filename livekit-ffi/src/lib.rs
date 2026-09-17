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

// `room-apis` and `core-modules` are two distinct FFI surfaces that ship as
// separate artifacts to different SDKs, and each sets up its own UniFFI
// scaffolding below. Selecting both is always a build-configuration mistake.
#[cfg(all(feature = "room-apis", feature = "core-modules"))]
compile_error!(
    "the `room-apis` and `core-modules` features are mutually exclusive. \
     Build livekit-ffi/core-modules with \
     `--no-default-features --features core-modules`."
);

// Both surfaces export `build_version` through the scaffolding below, which
// only exists when a surface is selected. Failing here beats failing inside
// the uniffi macro expansion.
#[cfg(not(any(feature = "room-apis", feature = "core-modules")))]
compile_error!(
    "exactly one FFI surface must be selected: enable either `room-apis` \
     (the default) or `core-modules`."
);

// Each surface owns everything specific to it, including its own types and
// statics; the globs republish it at the crate root. That keeps the public API
// (`livekit_ffi::{cabi, proto, server, FfiError, FFI_SERVER, ...}`) and the
// crate-internal `crate::` paths identical to before the modules existed.
#[cfg(feature = "room-apis")]
mod room_apis;
#[cfg(feature = "room-apis")]
pub use room_apis::*;

#[cfg(feature = "core-modules")]
mod core_modules;
#[cfg(feature = "core-modules")]
pub use core_modules::*;

/// Information about the build such as version.
///
/// Shared by both surfaces, and unconditional for that reason: a
/// room-apis-only build would otherwise ship a UniFFI component with no
/// exports at all (ffi-builds.yml greps the generated Python for it).
pub mod build_info;

// Must sit at the crate root: this defines `crate::UniFfiTag`, which every
// `#[uniffi::export]` in the surface modules resolves against. Only one call
// may exist per crate, which is why the two surfaces are mutually exclusive;
// they are kept separate so either can pin its own namespace later.
#[cfg(feature = "room-apis")]
uniffi::setup_scaffolding!("livekit_ffi");
#[cfg(feature = "core-modules")]
uniffi::setup_scaffolding!("livekit_ffi");
