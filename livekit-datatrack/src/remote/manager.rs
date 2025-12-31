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
    dtp::TrackHandle, remote::pipeline::RemoteTrackTask, DataTrack, DataTrackInfo,
    DecryptionProvider, InternalError, Remote, RemoteDataTrack, RemoteTrackInner,
};
use anyhow::Context;
use bytes::Bytes;
use from_variants::FromVariants;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::{wrappers::ReceiverStream, Stream};

/// An external event handled by [`Manager`].
#[derive(Debug, Clone, FromVariants)]
pub enum InputEvent {
    PublicationsUpdated(PublicationsUpdatedEvent),
    SubscriberHandles(SubscriberHandlesEvent),
    /// Packet has been received over the transport.
    PacketReceived(Bytes),
    /// Shutdown the manager, ending any subscriptions.
    Shutdown,
}

/// An event produced by [`Manager`] requiring external action.
#[derive(Debug, Clone, FromVariants)]
pub enum OutputEvent {
    SubscriptionUpdated(SubscriptionUpdatedEvent),
    /// Remote track has been published and a track object has been created for
    /// the user to interact with.
    TrackAvailable(DataTrack<Remote>),
}

/// Track publications updated for a specific participant.
///
/// This is used to detect newly published tracks as well as
/// tracks that have been unpublished.
///
#[derive(Debug, Clone)]
pub struct PublicationsUpdatedEvent {
    /// Mapping between participant identity and data tracks published by that participant.
    pub tracks_by_participant: HashMap<String, Vec<DataTrackInfo>>,
}

/// Subscriber handles available or updated.
#[derive(Debug, Clone)]
pub struct SubscriberHandlesEvent {
    /// Mapping between track handles attached to incoming packets to the
    /// track SIDs they belong to.
    pub mapping: HashMap<TrackHandle, String>,
}

/// User subscribed or unsubscribed to a track.
#[derive(Debug, Clone)]
pub struct SubscriptionUpdatedEvent {
    /// Identifier of the affected track.
    pub track_sid: String,
    /// Whether to subscribe or unsubscribe.
    pub subscribe: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TrackState {
    Available,
    Subscribed, // could include subscription details
    Unpublished,
}

impl TrackState {
    pub fn is_published(&self) -> bool {
        !matches!(self, Self::Unpublished)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TrackSubscriptionEvent {
    Subscribe, // TODO: include options
    Unsubscribe,
}

#[derive(Debug)]
pub struct ManagerOptions {
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
}

/// Manager for remote data tracks.
pub struct Manager {
    event_in_tx: mpsc::Sender<InputEvent>,
}

impl Manager {
    /// Creates a new manager with the specified options.
    pub fn new(options: ManagerOptions) -> (Self, ManagerTask, impl Stream<Item = OutputEvent>) {
        let (event_in_tx, event_in_rx) = mpsc::channel(Self::INPUT_BUFFER_SIZE);
        let (event_out_tx, event_out_rx) = mpsc::channel(Self::OUTPUT_BUFFER_SIZE);

        let manager = Manager { event_in_tx };
        let task = ManagerTask {
            decryption: options.decryption,
            event_in_rx,
            event_out_tx,
            descriptors: Default::default(),
        };

        let event_out_stream = ReceiverStream::new(event_out_rx);
        (manager, task, event_out_stream)
    }

    /// Handles an external event.
    pub fn handle_event(&self, event: InputEvent) -> Result<(), InternalError> {
        Ok(self.event_in_tx.try_send(event).context("Failed to send input event")?)
    }

    /// Number of [`InputEvent`]s to buffer.
    const INPUT_BUFFER_SIZE: usize = 4;

    /// Number of [`OutputEvent`]s to buffer.
    const OUTPUT_BUFFER_SIZE: usize = 4;
}

#[derive(Debug)]
enum Descriptor {
    Available { info: DataTrackInfo, publisher_identity: String },
    Subscribed,
}

pub struct ManagerTask {
    decryption: Option<Arc<dyn DecryptionProvider>>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,

    // Mapping between SID and descriptor.
    descriptors: HashMap<String, ()>,
}

impl ManagerTask {
    pub async fn run(mut self) {
        while let Some(event) = self.event_in_rx.recv().await {
            if matches!(event, InputEvent::Shutdown) {
                break;
            }
            let Err(err) = self.handle_event(event) else { continue };
            log::error!("Failed to handle input event: {}", err);
        }
    }

    fn handle_event(&mut self, event: InputEvent) -> Result<(), InternalError> {
        match event {
            InputEvent::PublicationsUpdated(event) => self.handle_publications_updated(event),
            InputEvent::SubscriberHandles(event) => self.handle_subscriber_handles(event),
            InputEvent::PacketReceived(bytes) => self.handle_packet_received(bytes),
            _ => Ok(()),
        }
    }

    fn handle_publications_updated(
        &mut self,
        event: PublicationsUpdatedEvent,
    ) -> Result<(), InternalError> {
        //  HashMap<String, (&str, DataTrackInfo)>

        let tracks_by_sid: HashMap<&str, (&str, &DataTrackInfo)> = event
            .tracks_by_participant
            .iter()
            .map(|(participant_identity, tracks)| {
                tracks.iter().map(move |track_info| {
                    (track_info.sid.as_str(), (participant_identity.as_str(), track_info))
                })
            })
            .flatten()
            .collect();

        let existing_sids: HashSet<_> = self.descriptors.keys().map(|key| key.as_str()).collect();
        let update_sids: HashSet<_> = tracks_by_sid.keys().map(|key| *key).collect();

        for new_sid in update_sids.difference(&existing_sids) {
            let Some((publisher_identity, info)) = tracks_by_sid.remove(new_sid) else { continue };
            self.handle_track_published(publisher_identity, info);
        }
        for removed_sid in existing_sids.difference(&update_sids) {
            // TODO: remove descriptor, set state to invalidate object/task
        }
        Ok(())
    }

    fn handle_track_published(&mut self, publisher_identity: String, info: DataTrackInfo) {
        let (packet_tx, packet_rx) = mpsc::channel(4); // TODO: tune
        let (frame_tx, frame_rx) = broadcast::channel(4);

        let info = Arc::new(info);
        let task = RemoteTrackTask {
            decryption: self.decryption.clone(),
            info: info.clone(),
            packet_rx,
            frame_tx,
        };
        livekit_runtime::spawn(task.run());
        // - create & store descriptor
        // include publisher identity and packet_tx

        let inner = RemoteTrackInner { frame_rx };
        let track = RemoteDataTrack::new(info, inner);
        _ = self.event_out_tx.send(track.into());
    }

    fn handle_track_unpublished(&mut self, track_sid: String) {
        // - end track task, invalidate object
    }

    fn handle_subscriber_handles(
        &mut self,
        event: SubscriberHandlesEvent,
    ) -> Result<(), InternalError> {
        todo!()
    }

    fn handle_packet_received(&mut self, bytes: Bytes) -> Result<(), InternalError> {
        // Decode packet
        // Lookup handle
        // Forward
        todo!()
    }
}
