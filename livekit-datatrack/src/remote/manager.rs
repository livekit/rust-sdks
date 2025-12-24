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
    dtp::TrackHandle, DataTrack, DataTrackInfo, DecryptionProvider, InternalError, Remote,
};
use anyhow::Context;
use bytes::Bytes;
use from_variants::FromVariants;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};

/// An external event handled by [`Manager`].
#[derive(Debug, Clone, FromVariants)]
pub enum InputEvent {
    PublicationsUpdated(PublicationsUpdatedEvent),
    SubscriberHandles(SubscriberHandlesEvent),
    /// Packet has been received over the transport.
    PacketReceived(Bytes),
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

#[derive(Debug)]
pub struct ManagerOptions {
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
}

/// Manager for remote data tracks.
pub struct Manager {
    event_in_tx: mpsc::Sender<InputEvent>, // sub request
}

impl Manager {
    const CH_BUFFER_SIZE: usize = 4;

    /// Creates a new manager with the specified options.
    pub fn new(options: ManagerOptions) -> (Self, ManagerTask, impl Stream<Item = OutputEvent>) {
        let (event_in_tx, event_in_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (event_out_tx, event_out_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);

        let manager = Manager { event_in_tx };
        let task = ManagerTask { decryption: options.decryption, event_in_rx, event_out_tx };

        let event_out_stream = ReceiverStream::new(event_out_rx);
        (manager, task, event_out_stream)
    }

    /// Handles an external event.
    pub fn handle_event(&self, event: InputEvent) -> Result<(), InternalError> {
        Ok(self.event_in_tx.try_send(event).context("Failed to send input event")?)
    }
}

pub struct ManagerTask {
    decryption: Option<Arc<dyn DecryptionProvider>>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,
}

impl ManagerTask {
    pub async fn run(mut self) {
        // TODO: check cancellation
        while let Some(event) = self.event_in_rx.recv().await {
            let Err(err) = self.handle_event(event) else { continue };
            log::error!("Failed to handle input event: {}", err);
        }
    }

    fn handle_event(&mut self, event: InputEvent) -> Result<(), InternalError> {
        match event {
            InputEvent::PublicationsUpdated(event) => self.handle_publications_updated(event),
            InputEvent::SubscriberHandles(event) => self.handle_subscriber_handles(event),
            InputEvent::PacketReceived(bytes) => self.handle_packet_received(bytes),
        }
    }

    fn handle_publications_updated(
        &mut self,
        event: PublicationsUpdatedEvent,
    ) -> Result<(), InternalError> {
        todo!()
    }

    fn handle_subscriber_handles(
        &mut self,
        event: SubscriberHandlesEvent,
    ) -> Result<(), InternalError> {
        todo!()
    }

    fn handle_packet_received(&mut self, bytes: Bytes) -> Result<(), InternalError> {
        todo!()
    }
}
