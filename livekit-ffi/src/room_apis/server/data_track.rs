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

use crate::FfiHandleId;
use bytes::Bytes;
use futures_util::StreamExt;
use livekit::data_track::{
    self as dt, DataTrackFrameEncoding, DataTrackSchemaEncoding, DataTrackSid,
    DataTrackSubscribeError, PushFrameErrorReason,
};
use std::sync::OnceLock;
use tokio::sync::Mutex;

// TODO: core_modules registers this converter identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module. Registering it here is
// safe meanwhile: the two surfaces are mutually exclusive, so only one is ever built.
uniffi::custom_type!(DataTrackSid, String, {
    remote,
    lower: |s| String::from(s),
    try_lift: |s| DataTrackSid::try_from(s).map_err(|e| uniffi::deps::anyhow::anyhow!("{e}")),
});

// TODO: core_modules defines this record identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module.
/// Information about a published data track.
#[derive(uniffi::Record)]
pub struct DataTrackInfo {
    /// Assigned by the SFU, and reassigned when the publisher reconnects. [`Self::name`]
    /// is the identifier that holds still.
    pub sid: DataTrackSid,
    pub name: String,
    pub uses_e2ee: bool,
    pub schema: Option<DataTrackSchemaId>,
    pub frame_encoding: Option<DataTrackFrameEncoding>,
}

impl From<&dt::DataTrackInfo> for DataTrackInfo {
    fn from(info: &dt::DataTrackInfo) -> Self {
        Self {
            sid: info.sid(),
            name: info.name().to_string(),
            uses_e2ee: info.uses_e2ee(),
            schema: info.schema().cloned().map(Into::into),
            frame_encoding: info.frame_encoding().cloned(),
        }
    }
}

// TODO: core_modules defines this record identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module.
/// Identifier for a data track schema: a name and an encoding.
#[derive(uniffi::Record)]
pub struct DataTrackSchemaId {
    pub name: String,
    pub encoding: DataTrackSchemaEncoding,
}

impl From<DataTrackSchemaId> for dt::DataTrackSchemaId {
    fn from(source: DataTrackSchemaId) -> Self {
        dt::DataTrackSchemaId::new(source.name, source.encoding)
    }
}

impl From<dt::DataTrackSchemaId> for DataTrackSchemaId {
    fn from(source: dt::DataTrackSchemaId) -> Self {
        Self { name: source.name().to_string(), encoding: source.encoding().clone() }
    }
}

// TODO: core_modules defines this record identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module.
/// A frame published on a data track.
///
/// Mirrors [`dt::DataTrackFrame`], whose fields are private behind a builder.
#[derive(uniffi::Record)]
pub struct DataTrackFrame {
    pub payload: Bytes,
    pub user_timestamp: Option<u64>,
}

impl From<DataTrackFrame> for dt::DataTrackFrame {
    fn from(frame: DataTrackFrame) -> Self {
        let inner = dt::DataTrackFrame::new(frame.payload);
        match frame.user_timestamp {
            Some(ts) => inner.with_user_timestamp(ts),
            None => inner,
        }
    }
}

impl From<dt::DataTrackFrame> for DataTrackFrame {
    fn from(frame: dt::DataTrackFrame) -> Self {
        Self { payload: frame.payload(), user_timestamp: frame.user_timestamp() }
    }
}

/// FFI wrapper around [`dt::LocalDataTrack`].
#[derive(uniffi::Object)]
pub struct LocalDataTrack {
    pub inner: dt::LocalDataTrack,
    /// Reached by [`crate::migration::data_track`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}

/// FFI wrapper around [`dt::RemoteDataTrack`].
#[derive(uniffi::Object)]
pub struct RemoteDataTrack {
    pub inner: dt::RemoteDataTrack,
    /// Reached by [`crate::migration::data_track`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}

#[uniffi::export(async_runtime = "tokio")]
impl LocalDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.inner.is_published()
    }

    /// Waits until the track is unpublished, returning immediately if it already is.
    pub async fn wait_for_unpublish(&self) {
        self.inner.wait_for_unpublish().await
    }

    /// Information about the data track.
    ///
    /// Read rather than cached: the SID is reassigned on a publisher reconnect, and the
    /// value handed over with the protobuf handle is a snapshot from that moment.
    pub fn info(&self) -> DataTrackInfo {
        self.inner.info().into()
    }

    /// Stops publishing the track.
    pub fn unpublish(&self) {
        self.inner.unpublish()
    }

    /// Try pushing a frame to subscribers of the track.
    ///
    /// The frame is lowered by copy, so the caller still holds its own value and can retry
    /// with it directly; only the reason comes back.
    pub fn try_push(&self, frame: DataTrackFrame) -> Result<(), PushFrameErrorReason> {
        self.inner.try_push(frame.into()).map_err(|err| err.reason())
    }
}

// TODO: core_modules defines this record identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module.
/// Track-level options for the incoming-frame pipeline.
///
/// Mirrors [`dt::RemoteDataTrackPipelineOptions`], whose fields are private behind a builder.
#[derive(uniffi::Record)]
pub struct RemoteDataTrackPipelineOptions {
    /// Maximum number of partial frames the depacketizer tracks concurrently. Zero is
    /// clamped to one.
    #[uniffi(default = 1)]
    pub max_partial_frames: u32,
}

impl From<RemoteDataTrackPipelineOptions> for dt::RemoteDataTrackPipelineOptions {
    fn from(options: RemoteDataTrackPipelineOptions) -> Self {
        dt::RemoteDataTrackPipelineOptions::default()
            .with_max_partial_frames(options.max_partial_frames as usize)
    }
}

// TODO: core_modules defines this record identically; #1438 brings the two into one
// crate, after which both collapse into a single ungated module.
/// Subscription-level options.
///
/// Mirrors [`dt::DataTrackSubscribeOptions`], whose fields are private behind a builder.
#[derive(uniffi::Record)]
pub struct DataTrackSubscribeOptions {
    /// Maximum number of received frames buffered internally. Zero is clamped to one.
    #[uniffi(default = 16)]
    pub buffer_size: u32,
}

impl From<DataTrackSubscribeOptions> for dt::DataTrackSubscribeOptions {
    fn from(options: DataTrackSubscribeOptions) -> Self {
        dt::DataTrackSubscribeOptions::default().with_buffer_size(options.buffer_size as usize)
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl RemoteDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.inner.is_published()
    }

    /// Information about the data track.
    ///
    /// Read rather than cached: the SID is reassigned on a publisher reconnect, and the
    /// value handed over with the protobuf handle is a snapshot from that moment.
    pub fn info(&self) -> DataTrackInfo {
        self.inner.info().into()
    }

    /// Identity of the participant who published the track.
    pub fn publisher_identity(&self) -> String {
        self.inner.publisher_identity().to_string()
    }

    /// Waits until the track is unpublished, returning immediately if it already is.
    pub async fn wait_for_unpublish(&self) {
        self.inner.wait_for_unpublish().await
    }

    /// Configures the pipeline handling incoming packets for this track.
    ///
    /// Applies to all current and future subscriptions and may be set at any time; new
    /// options take effect with the next received packet.
    pub fn set_pipeline_options(&self, options: RemoteDataTrackPipelineOptions) {
        self.inner.set_pipeline_options(options.into())
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
}

// The protobuf path does not hand this out: its `subscribe` returns a handle before the
// subscription has resolved, so it gets `migration::data_track::DataTrackSubscription`
// instead and pumps frames out as events.
//
// TODO: core_modules defines this object identically; #1438 brings the two into one crate,
// after which both collapse. Harder than the records: the bridge methods differ.
/// A stream of frames received from a [`RemoteDataTrack`].
#[derive(uniffi::Object)]
pub struct DataTrackStream(Mutex<dt::DataTrackStream>);

impl DataTrackStream {
    fn new(stream: dt::DataTrackStream) -> Self {
        Self(Mutex::new(stream))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl DataTrackStream {
    /// Returns the next received frame, or `None` once the subscription has ended.
    pub async fn next(&self) -> Option<DataTrackFrame> {
        self.0.lock().await.next().await.map(Into::into)
    }
}
