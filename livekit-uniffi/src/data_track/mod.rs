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

//! Data tracks core functionality from the [`livekit-datatrack`] crate.
//!
//! At a high level, FFI clients integrate this by instantiating a [`local::LocalDataTrackManager`] and a
//! [`remote::RemoteDataTrackManager`] inside their implementation of `Room`, forwarding input events and handling
//! output events. Architecturally, the managers have no dependency on WebRTC or the signaling
//! client, allowing them to be wired up to the FFI client's own implementations of these components.
//!

pub mod common;
pub mod e2ee;
pub mod local;
pub mod remote;
