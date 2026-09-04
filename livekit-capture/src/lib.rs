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

//! Video capture for the LiveKit Rust SDK.
//!
//! A capture source produces video: pixel frames ([`pixel`]) or pre-encoded
//! access units ([`encoded`]). A pump drives a source and publishes its
//! output to an RTC video source. Ready-made sources live in [`sources`]
//! and can be enabled by their corresponding features.

pub mod encoded;
pub mod error;
pub mod pixel;
pub mod primitive;
pub mod pump;
pub mod sources;

#[cfg(any(feature = "source-clock", feature = "source-pattern"))]
mod renderer;
mod utils;
