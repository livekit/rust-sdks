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

use std::mem::ManuallyDrop;

use crate::{server::FfiHandle, FfiHandleId, FfiResult, FFI_SERVER};

/// A wrapper over an [`FfiHandleId`] whose state lives in the [`FFI_SERVER`]
/// handle map rather than in the wrapper itself.
///
/// This is what lets a UniFFI object and the legacy C ABI operate on one shared
/// value: the UniFFI object stores only the id, and resolves [`Self::Inner`] out
/// of the handle store on every call, exactly as the protobuf request handlers
/// in [`crate::server::requests`] do.
///
/// # Ownership
///
/// Ownership is RAII. [`Self::from_ffi_handle_id`] *adopts* a handle, and
/// implementors are expected to `impl Drop` and call
/// `FFI_SERVER.drop_handle(self.ffi_handle_id())` there. Rust cannot make `Drop`
/// part of a trait contract, so that is a convention this trait documents rather
/// than enforces.
///
/// Two escape hatches exist, and the distinction between them matters:
///
/// * [`Self::ffi_handle_id`] borrows the id. The caller may use it to address the
///   same state over the legacy C ABI, but must **not** free it — this value is
///   still the owner.
/// * [`Self::leak_ffi_handle_id`] consumes `self` and hands ownership out, so the
///   handle survives and the caller becomes responsible for freeing it (via
///   `livekit_ffi_drop_handle`).
pub trait BackedByFfiHandle: Sized {
    /// The type actually stored in the handle map.
    ///
    /// For the two surfaces to share state this must be the *identical* type the
    /// legacy handlers pass to [`crate::server::FfiServer::retrieve_handle`] —
    /// that lookup is a downcast, so a merely structurally-similar type resolves
    /// to `FfiError::InvalidRequest("handle is not a ...")`.
    type Inner: FfiHandle + Clone;

    /// Adopt an existing handle.
    ///
    /// This *takes ownership*: dropping the returned value drops the handle.
    fn from_ffi_handle_id(ffi_handle_id: FfiHandleId) -> Self;

    /// Borrow the handle id. Does **not** transfer ownership.
    fn ffi_handle_id(&self) -> FfiHandleId;

    /// Consume `self` and return the handle id *without* dropping the handle.
    ///
    /// Use this to move ownership back to the legacy C ABI. The caller must
    /// eventually free the handle.
    fn leak_ffi_handle_id(self) -> FfiHandleId {
        // Suppress the implementor's `Drop` (and so its `drop_handle` call)
        // while still reading the id out.
        let this = ManuallyDrop::new(self);
        this.ffi_handle_id()
    }

    /// Given an instance of [Self::Inner], stores the object into the handle map
    /// and returns a new instance of the [BackedByFfiHandle] wrapper type.
    fn from_inner(inner: Self::Inner) -> Self {
        let handle_id = FFI_SERVER.next_id();
        FFI_SERVER.store_handle(handle_id, inner);
        Self::from_ffi_handle_id(handle_id)
    }

    /// Resolve the backing value out of the handle store.
    ///
    /// The clone releases the dashmap shard's read guard before returning, so a
    /// caller that then locks an inner mutex is not holding two locks at once.
    /// This mirrors the `retrieve_handle(..)?.clone()` idiom in
    /// [`crate::server::requests`].
    fn inner(&self) -> FfiResult<Self::Inner> {
        FFI_SERVER
            .retrieve_handle::<Self::Inner>(self.ffi_handle_id())
            .map(|handle| Self::Inner::clone(&handle))
    }
}
