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

//! The UniFFI core surface.
//!
//! Business logic that client SDKs adopt incrementally over UniFFI: the Swift,
//! Android and Flutter SDKs already have their own WebRTC stack, so this
//! surface deliberately depends on none of `livekit`, `libwebrtc`, `soxr-sys`
//! or `imgproc` -- which is what lets it build for visionOS, tvOS and Mac
//! Catalyst.
//!
//! Gated behind the `core-modules` feature, which is mutually exclusive with
//! `room-apis`.

/// Data tracks core from [`livekit-datatrack`].
pub mod data_track;

/// Data streams v2 core from [`livekit-data-stream`].
pub mod data_stream;

/// Access token generation and verification from [`livekit-api::access_token`].
pub mod access_token;

/// Forward log messages from Rust.
pub mod log_forward;

/// Shared exports and utilities.
pub mod common;

/// Global async runtime.
pub mod runtime;

// Forces livekit-net's UniFFI scaffolding into the cdylib. Nothing in this
// crate names livekit-net by path, so without this its exports go missing.
extern crate livekit_net;
