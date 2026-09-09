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

pub(crate) trait HasFfiHandleId {
    fn arc_from_self(self) -> ::std::sync::Arc<Self>
    where
        Self: Sized,
    {
        ::std::sync::Arc::new(self)
    }
}

/// This macro implements a bridge between the FFI handle system and the uniffi system.
///
/// It depends on migrating structs to maintain its own `handle_id` field. Migrating structs then expose
/// methods via `#[uniffi::export]`.
///
/// This macro generates foreign language methods:
/// 1) to get the `handle_id` so it can be passed via the old FFI system
/// 2) to construct an `Arc<Self>` from a `handle_id`
/// 3) to take ownership of an `Arc<Self>` from a `handle_id` (removing it from the FFI server)
///
/// It also provides:
/// 1) an `arc_to_self` method, which should be called at the end of the the migrating struct's constructor
/// to store the `Arc<Self>` in the FFI server and return it.
/// 2) a `Drop` implementation that removes the handle from the FFI server when the struct is dropped.
///
/// Migrating structs that already have a Drop implementation should consider wrapping the implementation into
/// an Inner struct, then implementing the Drop trait on the inner.
#[macro_export]
macro_rules! migrate_from_ffi {
    ($T:ty) => {
        impl crate::server::FfiHandle for ::std::sync::Arc<$T> {}

        impl crate::migration::HasFfiHandleId for $T {
            fn arc_from_self(mut self) -> ::std::sync::Arc<Self> {
                let handle_id = self.handle_id.unwrap_or_else(|| crate::FFI_SERVER.next_id());
                self.handle_id = Some(handle_id);
                let arc = ::std::sync::Arc::new(self);
                crate::FFI_SERVER.store_handle(handle_id, arc.clone());
                arc
            }
        }

        #[::uniffi::export]
        impl $T {
            pub fn ffi_handle_id(&self) -> crate::FfiHandleId {
                self.handle_id.expect(
                    "MIGRATION BUG: did you call arc_from_self() when creating this object?",
                )
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

            pub fn take_ffi_handle_id(
                &self,
            ) -> ::std::result::Result<::std::sync::Arc<Self>, crate::migration::MigrationError>
            {
                let handle_id = self.ffi_handle_id();
                match crate::FFI_SERVER.take_handle::<::std::sync::Arc<Self>>(handle_id) {
                    Ok(arc) => Ok(arc.clone()),
                    Err(s) => Err(crate::migration::MigrationError::TakeFfiHandleIdError(format!(
                        "MigrationError for handle_id {handle_id}: {s:?}"
                    ))),
                }
            }
        }

        impl Drop for $T {
            fn drop(&mut self) {
                if let Some(handle_id) = self.handle_id {
                    // drop handle is idempotent, so we can call it even if the handle was already
                    // taken by take_ffi_handle_id()
                    crate::FFI_SERVER.drop_handle(handle_id);
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
