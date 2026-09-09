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

/// This macro implements a bridge between the FFI handle system and the uniffi system.
///
/// It depends on migrating structs carrying a `handle_id: OnceLock<crate::FfiHandleId>` field.
/// Migrating structs then expose methods via `#[uniffi::export]`.
///
/// This macro generates foreign language methods:
/// 1) to get the `handle_id` so it can be passed via the old FFI system
/// 2) to construct an `Arc<Self>` from a `handle_id`, so we can migrate from the old FFI system
/// to the new uniffi system.
/// 3) to take ownership of a `&self` from the old FFI system, so the FFI side can release its
/// ownership of the handle but allow the uniffi side to continue use the struct.
///
/// The FFI server is a second owner: it holds an `Arc<Self>` from the first `ffi_handle_id()` call
/// until `livekit_ffi_drop_handle` or `take_ffi_handle_id` releases it. A struct that never publishes
/// a handle is owned by the uniffi side alone and drops with its last `Arc`.
///
/// The FFI system and uniffi system can be interchanged, however, once removed (via [take_ffi_handle_id] or 
/// [livekit_ffi_drop_handle]), the FFI system cannot easily be used again for that handle id.
#[macro_export]
macro_rules! migrate_from_ffi {
    ($T:ty) => {
        impl crate::server::FfiHandle for ::std::sync::Arc<$T> {}

        #[::uniffi::export]
        impl $T {
            /// Publishes this struct to the FFI handle map and returns the id the FFI side
            /// knows it by.
            ///
            /// The first call hands the FFI side co-ownership; later calls return the same id
            /// without republishing. An id whose handle the FFI side has already released is
            /// stale, and passing it back over the FFI fails with "handle not found".
            pub fn ffi_handle_id(self: ::std::sync::Arc<Self>) -> crate::FfiHandleId {
                let handle_id = *self.handle_id.get_or_init(|| {
                    let handle_id = crate::FFI_SERVER.next_id();
                    // the FFI side co-owns from here; released by livekit_ffi_drop_handle
                    crate::FFI_SERVER.store_handle(handle_id, ::std::sync::Arc::clone(&self));
                    handle_id
                });
                handle_id
            }

            #[::uniffi::constructor]
            pub fn from_ffi_handle_id(
                handle_id: crate::FfiHandleId,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, crate::migration::MigrationError>
            {
                match crate::FFI_SERVER.retrieve_handle::<::std::sync::Arc<Self>>(handle_id) {
                    Ok(arc) => Ok(arc.clone()),
                    Err(s) => Err(crate::migration::MigrationError::FromFfiHandleIdError(format!(
                        "MigrationError for handle_id {handle_id}: {s:?}"
                    ))),
                }
            }

            /// Takes the FFI side's ownership of this struct, leaving the uniffi side as the
            /// only owner. The FFI side calls this when it is done with the handle.
            ///
            /// The release is final: the handle id is not reusable, a second call errors, and
            /// so does a call on a struct that was never published.
            pub fn take_ffi_handle_id(
                &self,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, crate::migration::MigrationError>
            {
                let handle_id = self.handle_id.get().ok_or_else(|| {
                    crate::migration::MigrationError::TakeFfiHandleIdError(
                        "MigrationError for handle_id: handle_id not set yet".to_owned(),
                    )
                })?;
                match crate::FFI_SERVER.take_handle::<::std::sync::Arc<Self>>(*handle_id) {
                    Ok(arc) => Ok(arc.clone()),
                    Err(s) => Err(crate::migration::MigrationError::TakeFfiHandleIdError(format!(
                        "MigrationError for handle_id {handle_id}: {s:?}"
                    ))),
                }
            }
        }
    };
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MigrationError {
    #[error("Invalid handle id: {0}")]
    FromFfiHandleIdError(String),
    #[error("Failed to take handle id: {0}")]
    TakeFfiHandleIdError(String),
}
