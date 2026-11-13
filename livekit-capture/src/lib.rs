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

//! Media capture and playout for the LiveKit Rust SDK.
//!
//! [`audio`] provides fixed 10 ms PCM sources, their pump, and playout sinks.
//! [`video`] provides pixel and pre-encoded sources and their matching pumps.
//! Sources run through media-specific pumps while sharing cancellation,
//! running handles, and terminal results in [`pump`].

pub mod audio;
pub mod error;
pub mod pump;
pub mod video;
