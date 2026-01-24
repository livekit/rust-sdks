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
    api::{DataTrackFrame, DataTrackInfo, DataTrackSid, RemoteDataTrack, SubscribeError},
    packet::Handle,
};
use bytes::Bytes;
use from_variants::FromVariants;
use std::collections::HashMap;
use tokio::sync::{broadcast, oneshot};

/// An external event handled by [`super::manager::Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    PublicationUpdates(PublicationUpdatesEvent),
    Subscribe(SubscribeEvent),
    SubscriberHandles(SubscriberHandlesEvent),
    /// Packet has been received over the transport.
    PacketReceived(Bytes),
    Unsubscribe(UnsubscribeEvent),
    /// Shutdown the manager, ending any subscriptions.
    Shutdown,
}

/// An event produced by [`super::manager::Manager`] requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    SubscriptionUpdated(SubscriptionUpdatedEvent),
    /// Remote track has been published and a track object has been created for
    /// the user to interact with.
    TrackAvailable(RemoteDataTrack),
}

// MARK: - Input events

/// Track publications by remote participants updated.
///
/// This is used to detect newly published tracks as well as
/// tracks that have been unpublished.
///
#[derive(Debug)]
pub struct PublicationUpdatesEvent {
    /// Mapping between participant identity and data tracks published by that participant.
    pub updates: HashMap<String, Vec<DataTrackInfo>>,
}

pub(super) type SubscribeResult = Result<broadcast::Receiver<DataTrackFrame>, SubscribeError>;

/// User requested to subscribe to a track.
#[derive(Debug)]
pub struct SubscribeEvent {
    /// Identifier of the track.
    pub(super) sid: DataTrackSid,
    /// Async completion channel.
    pub(super) result_tx: oneshot::Sender<SubscribeResult>,
}

/// Subscriber handles available or updated.
#[derive(Debug)]
pub struct SubscriberHandlesEvent {
    /// Mapping between track handles attached to incoming packets to the
    /// track SIDs they belong to.
    pub mapping: HashMap<Handle, DataTrackSid>,
}

/// Unsubscribe from a track.
#[derive(Debug)]
pub struct UnsubscribeEvent {
    /// Identifier of the track to unsubscribe from.
    pub(super) sid: DataTrackSid,
}

// MARK: - Output events

/// User subscribed or unsubscribed to a track.
#[derive(Debug)]
pub struct SubscriptionUpdatedEvent {
    /// Identifier of the affected track.
    pub sid: DataTrackSid,
    /// Whether to subscribe or unsubscribe.
    pub subscribe: bool,
}
