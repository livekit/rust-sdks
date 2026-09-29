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

use crate::{proto, FfiHandleId};
use bytes::Bytes;
use livekit::data_track::{self as dt, PushFrameErrorReason};
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, Notify};

// Borrowed rather than registered here: a remote type can carry only one converter per
// UniFFI component, and livekit-common owns it.
uniffi::use_remote_type!(livekit_common::Bytes);

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

#[uniffi::export]
impl LocalDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.inner.is_published()
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

#[uniffi::export]
impl RemoteDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.inner.is_published()
    }

    /// Identity of the participant who published the track.
    pub fn publisher_identity(&self) -> String {
        self.inner.publisher_identity().to_string()
    }

    /// Configures the pipeline handling incoming packets for this track.
    ///
    /// Applies to all current and future subscriptions and may be set at any time; new
    /// options take effect with the next received packet.
    pub fn set_pipeline_options(&self, options: RemoteDataTrackPipelineOptions) {
        self.inner.set_pipeline_options(options.into())
    }
}

/// FFI wrapper around [`dt::DataTrackStream`].
///
/// The read/notify pair and the event round-trip exist only because the FFI boundary is
/// sync-request/async-event. Over uniffi that collapses to an async `next()` owning the
/// stream directly.
///
/// TODO: delete this in favour of livekit-uniffi's DataTrackStream, which already has
/// that shape, when the two crates merge. Inverting this one in place would be rewriting
/// code the merge then throws away.
#[derive(uniffi::Object)]
pub struct DataTrackStream {
    pub(crate) notify_read: Arc<Notify>,
    // Populated with the end-of-stream event once the stream has ended.
    pub(crate) eos_event: Arc<OnceLock<proto::DataTrackStreamEos>>,
    /// Used to drop the associated task when self is dropped
    #[allow(dead_code)]
    pub(crate) drop_tx: oneshot::Sender<()>,
    /// Reached by [`crate::migration::data_track`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}
