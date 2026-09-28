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

//! The UniFFI-facing surface of `livekit-ffi`.
//!
//! Types here are the UniFFI counterparts of the protobuf request handlers in
//! [`crate::server::requests`]. They deliberately hold no state of their own:
//! each one is a thin wrapper over an [`crate::FfiHandleId`] whose backing value
//! lives in the [`crate::FFI_SERVER`] handle map, so the exact same value is
//! reachable from both this surface and the legacy C ABI. See
//! [`backed_by_ffi_handle::BackedByFfiHandle`].
//!
//! Nothing in here modifies the legacy C ABI; the two surfaces are additive
//! views onto one handle store.

pub mod backed_by_ffi_handle;
pub mod sox_resampler;
