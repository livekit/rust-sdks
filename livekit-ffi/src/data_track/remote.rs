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

use super::common::{DataTrackFrame, DataTrackInfo};
use std::sync::Arc;

use futures_util::StreamExt;
use livekit_datatrack::api::{DataTrack, DataTrackSubscribeError, Remote};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Data track published by the remote participant.
#[derive(uniffi::Object)]
pub struct RemoteDataTrack {
    inner: DataTrack<Remote>,
    // After the migration is complete, this field can be removed.
    #[cfg(feature = "room-apis")]
    handle_id: std::sync::OnceLock<crate::FfiHandleId>,
}
// After the migration is complete, this call can be removed.
#[cfg(feature = "room-apis")]
crate::migrate_from_ffi!(RemoteDataTrack);

impl RemoteDataTrack {
    pub(crate) fn new(inner: DataTrack<Remote>) -> Self {
        Self {
            inner,
            #[cfg(feature = "room-apis")]
            handle_id: std::sync::OnceLock::new(),
        }
    }

    /// The wrapped track, for the room-apis request handlers. After the migration is complete,
    /// this can be removed.
    #[cfg(feature = "room-apis")]
    pub(crate) fn inner(&self) -> &DataTrack<Remote> {
        &self.inner
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RemoteDataTrack {
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

    /// Identity of the participant who published the track.
    pub fn publisher_identity(&self) -> String {
        self.inner.publisher_identity().to_string()
    }

    /// Subscribes to the data track.
    pub async fn subscribe(&self) -> Result<DataTrackStream, DataTrackSubscribeError> {
        self.inner.subscribe().await.map(DataTrackStream::new)
    }

    /// Subscribes to the data track with custom options.
    pub async fn subscribe_with_options(
        &self,
        options: DataTrackSubscribeOptions,
    ) -> Result<DataTrackStream, DataTrackSubscribeError> {
        self.inner.subscribe_with_options(options.into()).await.map(DataTrackStream::new)
    }

    /// Configures options for the pipeline handling incoming packets for this track.
    ///
    /// These options apply to all current and future subscriptions of this track, and may be
    /// set at any time. New options take affect with the next received packet.
    ///
    pub fn set_pipeline_options(&self, options: RemoteDataTrackPipelineOptions) {
        self.inner.set_pipeline_options(options.into())
    }
}

#[derive(uniffi::Record)]
pub struct DataTrackSubscribeOptions {
    /// Maximum number of received frames buffered internally. Zero is clamped to one.
    #[uniffi(default = 16)]
    pub buffer_size: u32,
}

impl From<DataTrackSubscribeOptions> for livekit_datatrack::api::DataTrackSubscribeOptions {
    fn from(options: DataTrackSubscribeOptions) -> Self {
        livekit_datatrack::api::DataTrackSubscribeOptions::default()
            .with_buffer_size(options.buffer_size as usize)
    }
}

/// FFI wrapper around [`livekit_datatrack::api::RemoteDataTrackPipelineOptions`]. The underlying
/// type uses the builder pattern with private fields.
///
/// An unset field takes the core type's default, so that value lives only in `livekit-datatrack`.
///
#[derive(uniffi::Record)]
pub struct RemoteDataTrackPipelineOptions {
    /// Maximum number of partial frames the depacketizer will track concurrently for this track.
    ///
    /// Higher values give more out-of-order tolerance for high-frequency senders at the cost of
    /// additional buffering. Zero is clamped to one.
    ///
    #[uniffi(default)]
    pub max_partial_frames: Option<u32>,
}

impl From<RemoteDataTrackPipelineOptions>
    for livekit_datatrack::api::RemoteDataTrackPipelineOptions
{
    fn from(options: RemoteDataTrackPipelineOptions) -> Self {
        let mut core = Self::default();
        if let Some(max_partial_frames) = options.max_partial_frames {
            core = core.with_max_partial_frames(max_partial_frames as usize);
        }
        core
    }
}

/// A stream of [`DataTrackFrame`]s received from a [`RemoteDataTrack`].
#[derive(uniffi::Object)]
pub struct DataTrackStream {
    /// The core stream, or `None` once the stream was closed.
    inner: Arc<Mutex<Option<livekit_datatrack::api::DataTrackStream>>>,
    closed: CancellationToken,
}

impl DataTrackStream {
    fn new(stream: livekit_datatrack::api::DataTrackStream) -> Self {
        Self { inner: Arc::new(Mutex::new(Some(stream))), closed: CancellationToken::new() }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl DataTrackStream {
    /// Returns the next received frame, or `None` once the subscription has ended or the stream
    /// was closed.
    pub async fn next(&self) -> Option<DataTrackFrame> {
        let mut stream = self.inner.lock().await;
        let stream = stream.as_mut()?;
        tokio::select! {
            biased;
            _ = self.closed.cancelled() => None,
            frame = stream.next() => frame.map(Into::into),
        }
    }

    /// Stops reading the stream and drops it, which ends the subscription once no other stream
    /// of the track remains.
    ///
    /// A pending or later `next()` returns `None`. Idempotent, and returns at once, so it is safe
    /// to call from a cancellation handler.
    pub fn close(&self) {
        if self.closed.is_cancelled() {
            return;
        }
        self.closed.cancel();
        // A read in progress holds the lock until it sees the cancellation. Drop the stream after
        // it in a task, because `close` must not block its caller.
        let inner = self.inner.clone();
        crate::runtime::runtime().spawn(async move {
            inner.lock().await.take();
        });
    }
}
