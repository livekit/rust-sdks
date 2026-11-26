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
//
// `__room-surface` is the room surface itself. Both room builds select it:
// `room-apis` with the bundled network stack, and `room-apis-host-transport`
// with the network that the host registers. The gates in this crate name
// `__room-surface`, so the two room builds compile the same code.
#[cfg(all(feature = "__room-surface", feature = "core-modules"))]
compile_error!(
    "a room feature (`room-apis` or `room-apis-host-transport`) and \
     `core-modules` are mutually exclusive. \
     Build livekit-ffi/core-modules with \
     `--no-default-features --features core-modules`."
);

// Both surfaces export `build_version` through the scaffolding below, which
// only exists when a surface is selected. Failing here beats failing inside
// the uniffi macro expansion.
#[cfg(not(any(feature = "__room-surface", feature = "core-modules")))]
compile_error!(
    "exactly one FFI surface must be selected: enable either `room-apis` \
     (the default), `room-apis-host-transport` or `core-modules`."
);

// `room-apis-host-transport` promises an artifact with no bundled WebSocket,
// HTTP or TLS client. Cargo features only add, so `room-apis` beside it links
// that stack again and the host clients become an override, not the only
// path. The TLS features, `tracing` and `capture` all imply `room-apis`, so
// one extra feature on the command line is enough to break the promise
// without a message. Stop the build instead.
#[cfg(all(feature = "room-apis", feature = "room-apis-host-transport"))]
compile_error!(
    "`room-apis-host-transport` excludes the bundled network stack, and \
     `room-apis` (or a feature that implies it, such as a TLS feature) links \
     it again. Build the host-transport artifact with \
     `--no-default-features --features room-apis-host-transport`."
);

// Each surface owns everything specific to it, including its own types and
// statics; the globs republish it at the crate root. That keeps the public API
// (`livekit_ffi::{cabi, proto, server, FfiError, FFI_SERVER, ...}`) and the
// crate-internal `crate::` paths identical to before the modules existed.
#[cfg(feature = "__room-surface")]
mod migration;
#[cfg(all(test, feature = "__room-surface"))]
mod migration_tests;

#[cfg(feature = "__room-surface")]
mod room_apis;
#[cfg(feature = "__room-surface")]
pub use room_apis::*;

// `conversion` is not part of the public API, so the glob above does not reach
// it. Re-export it for this crate, which keeps the `crate::conversion` paths
// that the surface used before it moved into `room_apis`.
#[cfg(feature = "__room-surface")]
pub(crate) use room_apis::conversion;

#[cfg(feature = "core-modules")]
mod core_modules;
#[cfg(feature = "core-modules")]
pub use core_modules::*;

// Shared stream value/handle wrappers; the managers remain exclusive to core-modules.
#[cfg(feature = "__room-surface")]
#[path = "core_modules/data_stream/mod.rs"]
pub mod data_stream;

/// Information about the build such as version.
///
/// Compiled under both the `room-apis` and `core-modules` features, and
/// unconditional for that reason: a room-apis-only build would otherwise ship
/// a UniFFI component with no exports at all (ffi-builds.yml greps the
/// generated Python for it).
pub mod build_info;

/// Shared UniFFI type registrations.
pub mod common;

/// Global async runtime.
pub(crate) mod runtime;

/// Data track objects from [`livekit-datatrack`].
///
/// Compiled under both the `room-apis` and `core-modules` features, so client SDKs get the same
/// track and stream objects whichever FFI surface they build against.
pub mod data_track;

// The host-transport build needs no `extern crate livekit_net` here, unlike
// `core_modules`. livekit-net is linked through `livekit`, and its UniFFI
// symbols and metadata reach the artifact without a path that names it.
// Checked on the macos-arm64 cdylib: 31 `uniffi_livekit_net_*` exports.

// Must sit at the crate root: this defines `crate::UniFfiTag`, which every
// `#[uniffi::export]` in the surface modules resolves against. Only one call
// may exist per crate, which is why the two surfaces are mutually exclusive;
// they are kept separate so either can pin its own namespace later.
#[cfg(feature = "__room-surface")]
use livekit::prelude::{ParticipantIdentity, ParticipantSid, TrackSid};
#[cfg(feature = "__room-surface")]
use livekit_common::ClientCapability;
#[cfg(feature = "__room-surface")]
use room_apis::session::{core_objects::*, *};
#[cfg(feature = "__room-surface")]
uniffi::include_scaffolding!("core_objects");
#[cfg(feature = "core-modules")]
uniffi::setup_scaffolding!("livekit_ffi");
