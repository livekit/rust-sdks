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

//! Data stream objects from the [`livekit-data-stream`] crate.
//!
//! Exported by both surfaces, so a client SDK sees the same readers and writers whichever one it
//! builds. Only how they are obtained differs: core-modules SDKs drive the data stream managers
//! themselves, while room-apis SDKs receive readers from the room and open writers through the
//! local participant.
//!

pub mod common;
pub mod incoming;
pub mod outgoing;
