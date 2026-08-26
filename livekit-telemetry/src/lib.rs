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

#![doc = include_str!("../README.md")]

/// Event data model: what SDKs push in.
mod event;

/// Bounded in-memory queue between `emit` and the exporter.
mod store;

/// Batch exporter actor: timer, OTLP encoding, retry policy.
mod exporter;

/// OTLP/HTTP protobuf encoding of a batch.
mod otlp;

/// Queue of encoded batches between the exporter and the transport (memory or disk).
mod cache;

/// OTLP protobuf types (re-exported from `opentelemetry-proto`).
mod proto;

/// Transport seam: how encoded batches leave the device.
mod transport;

/// Entry point and configuration.
mod telemetry;

pub use cache::{BatchCache, FileCache, MemoryCache};
pub use event::*;
pub use exporter::Exporter;
pub use telemetry::*;
pub use transport::*;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();
