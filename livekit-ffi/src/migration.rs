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
/// The FFI server is a second owner: it holds an `Arc<Self>` from an `ffi_handle_id()` call
/// until `livekit_ffi_drop_handle` or `take_ffi_handle_id` releases it. A struct that never publishes
/// a handle is owned by the uniffi side alone and drops with its last `Arc`.
///
/// The FFI system and uniffi system can be interchanged in either direction: once the FFI side has
/// released a handle (via [take_ffi_handle_id] or [livekit_ffi_drop_handle]), publishing the struct
/// again restores it to the handle map under the same id.
///
/// # Naming, and the one step cutover still costs
///
/// A migrating struct is scaffolding: the permanent object belongs in the crate that owns the
/// domain type, and this one goes away with the protobuf path. It carries the permanent object's
/// name anyway — `LocalDataTrack`, not `FfiLocalDataTrack` — so foreign SDK source keeps compiling
/// across the swap. The livekit types it wraps come in under a module alias to leave the name free.
///
/// The namespace is what the rename cannot reach. A uniffi object belongs to the crate that
/// *defines* it, not the one that hands it out, so a struct migrated here reaches an SDK as
/// `livekit_ffi.LocalDataTrack` and its permanent replacement, defined alongside the domain type,
/// as `livekit_datatrack.LocalDataTrack`. Cutting over is therefore exactly one import change per
/// SDK — a Python import, a Kotlin package, a Swift module — and nothing below it.
///
/// Two things are worth knowing before trying to remove that step.
///
/// Renaming through `#[uniffi(name = "...")]` does not work here: it leaves the foreign name out of
/// step with the `Arc<Self>` that [from_ffi_handle_id] returns, and bindgen rejects the pair with
/// "Constructor return type must be Self or Arc<Self>". Renaming the Rust struct is the only route.
///
/// Removing the import change means not defining the object here at all: the permanent object goes
/// into its owning crate up front, and this crate exports only free functions turning a handle id
/// into one. That works — a `#[uniffi::export]` fn here can return an object another crate defines
/// — but it makes every migration wait on its crate's uniffi surface landing first, which is the
/// coupling this macro exists to avoid. The import change buys each type the right to migrate alone.
#[macro_export]
macro_rules! migrate_from_ffi {
    ($T:ty) => {
        impl crate::server::FfiHandle for ::std::sync::Arc<$T> {}

        #[::uniffi::export]
        impl $T {
            /// Publishes this struct to the FFI handle map and returns the id the FFI side
            /// knows it by.
            ///
            /// Every call hands the FFI side co-ownership, under the id minted by the first one,
            /// so a struct the FFI side has released can be published again and keep its id.
            pub fn ffi_handle_id(self: ::std::sync::Arc<Self>) -> crate::FfiHandleId {
                let handle_id = *self.handle_id.get_or_init(|| crate::FFI_SERVER.next_id());
                // the FFI side co-owns from here; released by livekit_ffi_drop_handle
                crate::FFI_SERVER.store_handle(handle_id, ::std::sync::Arc::clone(&self));
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
            /// The id outlives the release: [ffi_handle_id] publishes the struct again under it.
            /// Until then the handle is absent from the map, so a second call errors, as does a
            /// call on a struct that was never published.
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

/// Everything holding a migrating type onto the protobuf path, one file per type, kept
/// apart from the type itself so that each goes in one piece when its path does.
///
/// What cannot live here is the `handle_id` field the bridge needs, and the `pub(crate)`
/// on whatever internals these files reach. Both go the same way.
mod audio_resampler;
mod audio_stream;
mod data_track;
mod track;
mod video_stream;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MigrationError {
    #[error("Invalid handle id: {0}")]
    FromFfiHandleIdError(String),
    #[error("Failed to take handle id: {0}")]
    TakeFfiHandleIdError(String),
}
