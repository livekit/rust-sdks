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

//! OTLP protobuf types from the upstream `opentelemetry-proto` crate (prost-generated,
//! `gen-tonic-messages` only — no tonic/gRPC). The crate tracks a newer proto revision than
//! the OTLP 1.x stable surface, so construct its messages with `..Default::default()`.
//!
//! Only the types are used; the `opentelemetry`/`opentelemetry_sdk` crates it depends on are
//! dead code here and LTO removes them from release binaries (measured: +8 bytes on iOS).

pub mod opentelemetry {
    pub mod proto {
        pub use opentelemetry_proto::tonic::{collector, common, logs, resource};
    }
}
