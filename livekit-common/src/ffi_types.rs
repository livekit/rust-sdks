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

//! Shared UniFFI type registrations.
//!
//! A remote type such as [`bytes::Bytes`] can only be registered once per UniFFI component:
//! `custom_type!` emits a `public` converter per component, and the generated Swift files are
//! compiled into a single module, so a second registration fails to build. Registering it here
//! and having each component borrow it with `uniffi::use_remote_type!` keeps exactly one
//! declaration no matter how many crates need the type.
//!
//! Owning a registration requires being a UniFFI component: `custom_type!` needs
//! `crate::UniFfiTag`, and bindgen rejects type metadata that belongs to no namespace
//! (`Unknown namespace for CustomType`). Hence the `setup_scaffolding!` in `lib.rs`.

use bytes::Bytes;

uniffi::custom_type!(Bytes, Vec<u8>, { remote });
