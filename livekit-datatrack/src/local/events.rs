// Copyright 2025 LiveKit, Inc.
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

use std::sync::Arc;
use crate::{
    api::{DataTrackInfo, DataTrackOptions, LocalDataTrack, PublishError},
    packet::Handle,
};
use bytes::Bytes;
use from_variants::FromVariants;
use tokio::sync::oneshot;

/// An external event handled by [`super::manager::Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    PublishRequest(PublishRequest),
    PublishCancelled(PublishCancelled),
    QueryPublished(QueryPublished),
    UnpublishRequest(UnpublishRequest),
    SfuPublishResponse(SfuPublishResponse),
    SfuUnpublishResponse(SfuUnpublishResponse),
    /// Shutdown the manager and all associated tracks.
    Shutdown,
}

/// An event produced by [`super::manager::Manager`] requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    SfuPublishRequest(SfuPublishRequest),
    SfuUnpublishRequest(SfuUnpublishRequest),
    /// Serialized packets are ready to be sent over the transport.
    PacketsAvailable(Vec<Bytes>),
}

// MARK: - Input events

/// Client requested to publish a track.
#[derive(Debug)]
pub struct PublishRequest {
    /// Publish options.
    pub(super) options: DataTrackOptions,
    /// Async completion channel.
    pub(super) result_tx: oneshot::Sender<Result<LocalDataTrack, PublishError>>,
}

/// Client request to publish a track has been cancelled.
#[derive(Debug)]
pub struct PublishCancelled {
    /// Publisher handle of the pending publication.
    pub(super) handle: Handle,
}

/// Client request to unpublish a track.
#[derive(Debug)]
pub struct UnpublishRequest {
    /// Publisher handle of the track to unpublish.
    pub(super) handle: Handle,
}

/// SFU responded to a request to publish a data track.
#[derive(Debug)]
pub struct SfuPublishResponse {
    /// Publisher handle of the track.
    pub handle: Handle,
    /// Outcome of the publish request.
    pub result: Result<DataTrackInfo, PublishError>,
}

/// SFU notification that a track has been unpublished.
#[derive(Debug)]
pub struct SfuUnpublishResponse {
    /// Publisher handle of the track that was unpublished.
    pub handle: Handle,
}

/// Get information about all currently published tracks.
#[derive(Debug)]
pub struct QueryPublished {
    pub(super) result_tx: oneshot::Sender<Vec<Arc<DataTrackInfo>>>
}

// MARK: - Output events

/// Request sent to the SFU to publish a track.
#[derive(Debug)]
pub struct SfuPublishRequest {
    pub handle: Handle,
    pub name: String,
    pub uses_e2ee: bool,
}

/// Request sent to the SFU to unpublish a track.
#[derive(Debug)]
pub struct SfuUnpublishRequest {
    /// Publisher handle of the track to unpublish.
    pub handle: Handle,
}
