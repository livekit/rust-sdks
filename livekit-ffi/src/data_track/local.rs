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

use super::common::{DataTrackFrame, DataTrackInfo, DataTrackSchemaId};
use livekit_datatrack::api::{DataTrack, DataTrackFrameEncoding, Local, PushFrameErrorReason};

/// Data track published by the local participant.
#[derive(uniffi::Object)]
pub struct LocalDataTrack {
    inner: DataTrack<Local>,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}
// After the migration is complete, this call can be removed.
#[cfg(feature = "room-apis")]
crate::migrate_from_ffi!(LocalDataTrack);

impl LocalDataTrack {
    pub(crate) fn new(inner: DataTrack<Local>) -> Self {
        Self {
            inner,
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }

    /// The wrapped track, for the room-apis request handlers. After the migration is complete,
    /// this can be removed.
    #[cfg(feature = "room-apis")]
    pub(crate) fn inner(&self) -> &DataTrack<Local> {
        &self.inner
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl LocalDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.inner.is_published()
    }

    /// Waits asynchronously until the track is unpublished.
    ///
    /// Use this to trigger follow-up work once the track is no longer published.
    /// If the track is already unpublished, this method returns immediately.
    ///
    pub async fn wait_for_unpublish(&self) {
        self.inner.wait_for_unpublish().await
    }

    /// Information about the data track.
    pub fn info(&self) -> DataTrackInfo {
        self.inner.info().into()
    }

    /// Try pushing a frame to subscribers of the track.
    pub fn try_push(&self, frame: DataTrackFrame) -> Result<(), PushFrameErrorReason> {
        // `PushFrameError` returns ownership of the unpublished frame to the caller because Rust's
        // `try_push` moves it. Across the FFI boundary the frame is lowered by copy, so the caller
        // still holds its own value and can retry with it directly; just provide the reason.
        self.inner.try_push(frame.into()).map_err(|err| err.reason())
    }

    /// Unpublishes the track.
    pub fn unpublish(&self) {
        self.inner.unpublish();
    }
}

/// Options for publishing a data track.
///
/// FFI wrapper around [`livekit_datatrack::api::DataTrackOptions`]. The underlying type uses the
/// builder pattern with private fields.
///
#[derive(uniffi::Record)]
pub struct DataTrackOptions {
    pub name: String,
    #[uniffi(default)]
    pub schema: Option<DataTrackSchemaId>,
    #[uniffi(default)]
    pub frame_encoding: Option<DataTrackFrameEncoding>,
}

impl From<DataTrackOptions> for livekit_datatrack::api::DataTrackOptions {
    fn from(source: DataTrackOptions) -> Self {
        let mut options = livekit_datatrack::api::DataTrackOptions::new(source.name);
        if let Some(schema) = source.schema {
            options = options.with_schema(schema.into());
        }
        if let Some(frame_encoding) = source.frame_encoding {
            options = options.with_frame_encoding(frame_encoding);
        }
        options
    }
}
