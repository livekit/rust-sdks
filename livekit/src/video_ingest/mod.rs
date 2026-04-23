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

//! High-level helpers for ingesting pre-encoded video into a LiveKit room.
//!
//! This module hides the moving parts of pulling a pre-encoded bytestream
//! from a source (currently: TCP) and turning it into a published
//! LiveKit track. Callers configure a small options struct and hand off a
//! `Room`; the helper does the rest.
//!
//! See [`EncodedTcpIngest`] for the TCP-based helper.

#[cfg(not(target_arch = "wasm32"))]
mod demux;
#[cfg(not(target_arch = "wasm32"))]
mod encoded_tcp;
#[cfg(not(target_arch = "wasm32"))]
mod keyframe;

#[cfg(not(target_arch = "wasm32"))]
pub use encoded_tcp::{
    EncodedIngestObserver, EncodedIngestStats, EncodedTcpIngest, EncodedTcpIngestOptions,
};
