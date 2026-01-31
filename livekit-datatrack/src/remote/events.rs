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

/// An external event handled by [`Manager`](super::manager::Manager).
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    SubscribeRequest(SubscribeRequest),
    UnsubscribeRequest(UnsubscribeRequest),
    SfuPublicationUpdates(SfuPublicationUpdates),
    SfuSubscriberHandles(SfuSubscriberHandles),
    /// Packet has been received over the transport.
    PacketReceived(Bytes),
    /// Resend all subscription updates.
    ///
    /// This must be sent after a full reconnect to ensure the SFU knows which
    /// tracks are subscribed to locally.
    ///
    ResendSubscriptionUpdates,
    /// Shutdown the manager, ending any subscriptions.
    Shutdown,
}

/// An event produced by [`Manager`](super::manager::Manager) requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    SfuUpdateSubscription(SfuUpdateSubscription),
    /// A track has been published by a remote participant and is available to be
    /// subscribed to.
    ///
    /// Emit a public event to deliver the track to the user, allowing them to subscribe
    /// with [`RemoteDataTrack::subscribe`] if desired.
    ///
    TrackAvailable(RemoteDataTrack),
}

// MARK: - Input events

/// Result of a [`SubscribeRequest`].
pub(super) type SubscribeResult = Result<broadcast::Receiver<DataTrackFrame>, SubscribeError>;

/// Client requested to subscribe to a data track.
///
/// This is sent when the user calls [`RemoteDataTrack::subscribe`].
///
/// Only the first request to subscribe to a given track incurs meaningful overhead; subsequent
/// requests simply attach an additional receiver to the broadcast channel, allowing them to consume
/// frames from the existing subscription pipeline.
///
#[derive(Debug)]
pub struct SubscribeRequest {
    /// Identifier of the track.
    pub(super) sid: DataTrackSid,
    /// Async completion channel.
    pub(super) result_tx: oneshot::Sender<SubscribeResult>,
}

/// Client requested to unsubscribe from a data track.
#[derive(Debug)]
pub struct UnsubscribeRequest {
    /// Identifier of the track to unsubscribe from.
    pub(super) sid: DataTrackSid,
}

/// SFU notification that track publications have changed.
///
/// This event is produced from both [`livekit_protocol::JoinResponse`] and [`livekit_protocol::ParticipantUpdate`]
/// to provide a complete view of remote participants' track publications:
///
/// - From a `JoinResponse`, it captures the initial set of tracks published when a participant joins.
/// - From a `ParticipantUpdate`, it captures subsequent changes (i.e., new tracks being
///  published and existing tracks unpublished).
///
/// See [`event_from_join`](super::proto::event_from_join) and
///     [`event_from_participant_update`](super::proto::event_from_participant_update).
///
#[derive(Debug)]
pub struct SfuPublicationUpdates {
    /// Mapping between participant identity and data tracks currently
    /// published by that participant.
    pub updates: HashMap<String, Vec<DataTrackInfo>>,
}

/// SFU notification that handles have been assigned for requested subscriptions.
///
/// Protocol equivalent: [`livekit_protocol::DataTrackSubscriberHandles`].
///
#[derive(Debug)]
pub struct SfuSubscriberHandles {
    /// Mapping between track handles attached to incoming packets to the
    /// track SIDs they belong to.
    pub mapping: HashMap<Handle, DataTrackSid>,
}

// MARK: - Output events

/// Request sent to the SFU to update the subscription for a data track.
///
/// Protocol equivalent: [`livekit_protocol::UpdateDataSubscription`].
///
#[derive(Debug)]
pub struct SfuUpdateSubscription {
    /// Identifier of the affected track.
    pub sid: DataTrackSid,
    /// Whether to subscribe or unsubscribe.
    pub subscribe: bool,
}
