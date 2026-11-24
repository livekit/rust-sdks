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

//! Room-owned stream, data track and schema commands.

use super::{NativeRoomSession, NativeSessionError};
use crate::data_stream::{
    common::{
        ByteStreamInfo, DataStreamError, StreamByteOptions, StreamTextOptions, TextStreamInfo,
    },
    outgoing::{ByteStreamWriter, TextStreamWriter},
};
use crate::data_track::{
    common::DataTrackSchemaId,
    local::{DataTrackOptions, LocalDataTrack},
};
use bytes::Bytes;
use std::future::Future;

impl NativeRoomSession {
    async fn stream_command<
        T: Send + 'static,
        F: Future<Output = Result<T, DataStreamError>> + Send + 'static,
    >(
        &self,
        work: impl FnOnce(std::sync::Arc<livekit::Room>) -> F + Send + 'static,
    ) -> Result<T, DataStreamError> {
        let room = self.shared.room().map_err(|_| DataStreamError::AlreadyClosed)?;
        self.shared.command(work(room)).await.map_err(|_| DataStreamError::AlreadyClosed)?
    }
}

#[uniffi::export]
impl NativeRoomSession {
    /// Looks up a currently published remote data track from a snapshot SID.
    pub fn remote_data_track(
        &self,
        sid: String,
    ) -> Option<std::sync::Arc<crate::data_track::remote::RemoteDataTrack>> {
        self.shared.inner.lock().data_tracks.get(&sid).cloned()
    }

    /// Sends a complete text stream using the room's transport and recipient capabilities.
    pub async fn send_text(
        &self,
        text: String,
        options: StreamTextOptions,
    ) -> Result<TextStreamInfo, DataStreamError> {
        self.stream_command(move |room| async move {
            Ok(room.local_participant().send_text(&text, options.into()).await?.into())
        })
        .await
    }

    /// Sends a complete byte stream over the room's transport.
    pub async fn send_bytes(
        &self,
        data: Bytes,
        options: StreamByteOptions,
    ) -> Result<ByteStreamInfo, DataStreamError> {
        self.stream_command(move |room| async move {
            Ok(room.local_participant().send_bytes(data, options.into()).await?.into())
        })
        .await
    }

    /// Sends a file, with the core's progress and compression behavior.
    pub async fn send_file(
        &self,
        path: String,
        options: StreamByteOptions,
    ) -> Result<ByteStreamInfo, DataStreamError> {
        self.stream_command(move |room| async move {
            Ok(room.local_participant().send_file(path, options.into()).await?.into())
        })
        .await
    }

    /// Opens an incremental text writer belonging to the room.
    pub async fn stream_text(
        &self,
        options: StreamTextOptions,
    ) -> Result<TextStreamWriter, DataStreamError> {
        self.stream_command(move |room| async move {
            Ok(TextStreamWriter(room.local_participant().stream_text(options.into()).await?))
        })
        .await
    }

    /// Opens an incremental byte writer belonging to the room.
    pub async fn stream_bytes(
        &self,
        options: StreamByteOptions,
    ) -> Result<ByteStreamWriter, DataStreamError> {
        self.stream_command(move |room| async move {
            Ok(ByteStreamWriter(room.local_participant().stream_bytes(options.into()).await?))
        })
        .await
    }

    /// Publishes a data track through the authoritative room.
    pub async fn publish_data_track(
        &self,
        options: DataTrackOptions,
    ) -> Result<LocalDataTrack, livekit_datatrack::api::PublishError> {
        use livekit_datatrack::api::PublishError;
        let room = self.shared.room().map_err(|_| PublishError::Disconnected)?;
        self.shared
            .command(async move {
                room.local_participant().publish_data_track(options).await.map(LocalDataTrack::new)
            })
            .await
            .map_err(|_| PublishError::Disconnected)?
    }

    /// Defines a schema on the server before publishing a track that references it.
    pub async fn define_schema(
        &self,
        id: DataTrackSchemaId,
        definition: String,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        self.shared
            .command(async move {
                Ok(room.local_participant().define_schema(id.into(), definition).await?)
            })
            .await?
    }

    /// Fetches a schema defined by a participant in this room.
    pub async fn get_schema(
        &self,
        id: DataTrackSchemaId,
        participant: String,
    ) -> Result<String, NativeSessionError> {
        let room = self.shared.room()?;
        self.shared
            .command(async move {
                Ok(room.local_participant().get_schema(id.into(), participant.into()).await?)
            })
            .await?
    }
}
