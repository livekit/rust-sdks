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

use crate::{
    api::{DataTrackInfo, DataTrackOptions, LocalDataTrack, PublishError},
    packet::Handle,
};
use bytes::Bytes;
use from_variants::FromVariants;
use tokio::sync::oneshot;

// MARK: - Input events

/// An external event handled by [`super::manager::Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    Publish(PublishEvent),
    PublishResult(PublishResultEvent),
    PublishCancelled(PublishCancelledEvent),
    Unpublish(UnpublishEvent),
    /// Shutdown the manager and all associated tracks.
    Shutdown,
}

/// Request to publish a data track.
#[derive(Debug)]
pub struct PublishEvent {
    /// Publish options.
    pub(super) options: DataTrackOptions,
    /// Async completion channel.
    pub(super) result_tx: oneshot::Sender<Result<LocalDataTrack, PublishError>>,
}


/// Result of a publish request.
#[derive(Debug)]
pub struct PublishResultEvent {
    /// Publisher handle of the track.
    pub handle: Handle,
    /// Outcome of the publish request.
    pub result: Result<DataTrackInfo, PublishError>,
}

/// Request to publish a data track was cancelled.
#[derive(Debug)]
pub struct PublishCancelledEvent {
    /// Publisher handle of the pending publication.
    pub(super) handle: Handle,
}

/// Track has been unpublished.
#[derive(Debug)]
pub struct UnpublishEvent {
    /// Publisher handle of the track that was unpublished.
    pub handle: Handle,
    /// Whether the unpublish was initiated by the client.
    pub client_initiated: bool,
}

// MARK: - Output events

/// An event produced by [`super::manager::Manager`] requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    PublishRequest(PublishRequestEvent),
    UnpublishRequest(UnpublishRequestEvent),
    /// Serialized packets are ready to be sent over the transport.
    PacketsAvailable(Vec<Bytes>),
}

/// Local participant requested to publish a track.
#[derive(Debug)]
pub struct PublishRequestEvent {
    pub handle: Handle,
    pub name: String,
    pub uses_e2ee: bool,
}

/// Local participant unpublished a track.
///
/// This can either occur explicitly through user action or implicitly when the last
/// reference to the track is dropped.
///
#[derive(Debug)]
pub struct UnpublishRequestEvent {
    /// Publisher handle of the track to unpublish.
    pub handle: Handle,
}
