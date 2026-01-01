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
    dtp::{Dtp, TrackHandle},
    remote::pipeline::RemoteTrackTask,
    DataTrackFrame, DataTrackInfo, DecryptionProvider, InternalError, RemoteDataTrack,
    RemoteTrackInner, SubscribeError,
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use from_variants::FromVariants;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_stream::{wrappers::ReceiverStream, Stream};

/// An external event handled by [`Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    PublicationsUpdated(PublicationsUpdatedEvent),
    Subscribe(SubscribeEvent),
    SubscriberHandles(SubscriberHandlesEvent),
    /// Packet has been received over the transport.
    PacketReceived(Bytes),
    /// Shutdown the manager, ending any subscriptions.
    Shutdown,
}

/// An event produced by [`Manager`] requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    SubscriptionUpdated(SubscriptionUpdatedEvent),
    /// Remote track has been published and a track object has been created for
    /// the user to interact with.
    TrackAvailable(RemoteDataTrack),
}

/// Track publications updated for a specific participant.
///
/// This is used to detect newly published tracks as well as
/// tracks that have been unpublished.
///
#[derive(Debug)]
pub struct PublicationsUpdatedEvent {
    /// Mapping between participant identity and data tracks published by that participant.
    pub tracks_by_participant: HashMap<String, Vec<DataTrackInfo>>,
}

/// Subscriber handles available or updated.
#[derive(Debug)]
pub struct SubscriberHandlesEvent {
    /// Mapping between track handles attached to incoming packets to the
    /// track SIDs they belong to.
    pub mapping: HashMap<TrackHandle, String>,
}

/// User requested to subscribe to a track.
#[derive(Debug)]
pub struct SubscribeEvent {
    /// Identifier of the track.
    pub(super) track_sid: String,
    /// Async completion channel.
    pub(super) result_tx:
        oneshot::Sender<Result<broadcast::Receiver<DataTrackFrame>, SubscribeError>>,
}

/// User subscribed or unsubscribed to a track.
#[derive(Debug)]
pub struct SubscriptionUpdatedEvent {
    /// Identifier of the affected track.
    pub track_sid: String,
    /// Whether to subscribe or unsubscribe.
    pub subscribe: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TrackState {
    Published,
    Unpublished,
}

impl TrackState {
    pub fn is_published(&self) -> bool {
        !matches!(self, Self::Unpublished)
    }
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

        let manager = Manager { event_in_tx: event_in_tx.clone() };
        let task = ManagerTask {
            decryption: options.decryption,
            event_in_tx: event_in_tx.downgrade(),
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
struct Descriptor {
    info: Arc<DataTrackInfo>,
    state_tx: watch::Sender<TrackState>,
    state: DescriptorState,
}

#[derive(Debug)]
enum DescriptorState {
    Available,
    Pending {
        result_txs:
            Vec<oneshot::Sender<Result<broadcast::Receiver<DataTrackFrame>, SubscribeError>>>,
    },
    Subscribed {
        handle: TrackHandle,
        packet_tx: mpsc::Sender<Dtp>,
        frame_tx: broadcast::Sender<DataTrackFrame>,
    },
}

pub struct ManagerTask {
    decryption: Option<Arc<dyn DecryptionProvider>>,
    event_in_tx: mpsc::WeakSender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,

    /// Mapping between SID and descriptor.
    descriptors: HashMap<String, Descriptor>,
    // TODO: create index for fast lookup by handle
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
        self.shutdown();
    }

    fn handle_event(&mut self, event: InputEvent) -> Result<(), InternalError> {
        match event {
            InputEvent::PublicationsUpdated(event) => self.handle_publications_updated(event),
            InputEvent::SubscriberHandles(event) => self.handle_subscriber_handles(event),
            InputEvent::PacketReceived(bytes) => self.handle_packet_received(bytes),
            InputEvent::Subscribe(event) => self.handle_subscribe(event),
            _ => Ok(()),
        }
    }

    fn handle_publications_updated(
        &mut self,
        event: PublicationsUpdatedEvent,
    ) -> Result<(), InternalError> {
        // TODO: diff with descriptors
        // let tracks_by_sid: HashMap<&str, (&str, &DataTrackInfo)> = event
        //     .tracks_by_participant
        //     .iter()
        //     .map(|(participant_identity, tracks)| {
        //         tracks.iter().map(move |track_info| {
        //             (track_info.sid.as_str(), (participant_identity.as_str(), track_info))
        //         })
        //     })
        //     .flatten()
        //     .collect();

        // let existing_sids: HashSet<_> = self.descriptors.keys().map(|key| key.as_str()).collect();
        // let update_sids: HashSet<_> = tracks_by_sid.keys().map(|key| *key).collect();

        // for new_sid in update_sids.difference(&existing_sids) {
        //     let Some((publisher_identity, info)) = tracks_by_sid.remove(new_sid) else { continue };
        //     self.handle_track_published(publisher_identity, info);
        // }
        // for removed_sid in existing_sids.difference(&update_sids) {
        //     // TODO: remove descriptor, set state to invalidate object/task
        // }
        // Ok(())
        todo!()
    }

    fn handle_track_published(&mut self, publisher_identity: String, info: DataTrackInfo) {
        if self.descriptors.contains_key(&info.sid) {
            log::error!("Existing descriptor for track {}", info.sid);
            return;
        }
        let info = Arc::new(info);

        let (state_tx, state_rx) = watch::channel(TrackState::Published);
        let descriptor =
            Descriptor { info: info.clone(), state_tx, state: DescriptorState::Available };
        self.descriptors.insert(descriptor.info.sid.clone(), descriptor);

        let inner = RemoteTrackInner {
            state_rx,
            event_in_tx: self.event_in_tx.clone(),
            publisher_identity,
        };
        let track = RemoteDataTrack::new(info, inner);
        self.event_out_tx.send(track.into());
    }

    fn handle_track_unpublished(&mut self, track_sid: String) {
        let Some(descriptor) = self.descriptors.remove(&track_sid) else {
            log::error!("Unknown track {}", track_sid);
            return;
        };
        descriptor.state_tx.send(TrackState::Unpublished);
        // TODO: this should end the track task
    }

    fn handle_subscribe(&mut self, event: SubscribeEvent) -> Result<(), InternalError> {
        let Some(descriptor) = self.descriptors.get_mut(event.track_sid.as_str()) else {
            _ = event.result_tx.send(Err(SubscribeError::Internal(
                anyhow!("Cannot subscribe to unknown track").into(),
            )));
            return Ok(());
        };
        match &mut descriptor.state {
            DescriptorState::Available => {
                let update_event = SubscriptionUpdatedEvent {
                    track_sid: event.track_sid.to_string(),
                    subscribe: true,
                };
                _ = self.event_out_tx.send(update_event.into());
                descriptor.state = DescriptorState::Pending { result_txs: vec![event.result_tx] };
                // TODO: schedule timeout internally
            }
            DescriptorState::Pending { result_txs } => {
                result_txs.push(event.result_tx);
            }
            DescriptorState::Subscribed { handle: _, packet_tx: _, frame_tx } => {
                let frame_rx = frame_tx.subscribe();
                _ = event.result_tx.send(Ok(frame_rx))
            }
        }
        Ok(())
    }

    fn handle_subscriber_handles(
        &mut self,
        event: SubscriberHandlesEvent,
    ) -> Result<(), InternalError> {
        for (handle, sid) in event.mapping {
            self.register_subscriber_handle(handle, sid);
        }
        Ok(())
    }

    fn register_subscriber_handle(&mut self, handle: TrackHandle, sid: String) {
        let Some(descriptor) = self.descriptors.get_mut(&sid) else {
            log::warn!("Unknown track: {}", sid);
            return;
        };
        match &mut descriptor.state {
            DescriptorState::Available => log::warn!("No subscription"),
            DescriptorState::Pending { result_txs } => {
                let (packet_tx, packet_rx) = mpsc::channel(4); // TODO: tune
                let (frame_tx, frame_rx) = broadcast::channel(4);

                let track_task = RemoteTrackTask {
                    decryption: self.decryption.clone(),
                    info: descriptor.info.clone(),
                    state_rx: descriptor.state_tx.subscribe(),
                    packet_rx,
                    frame_tx: frame_tx.clone(),
                    event_out_tx: self.event_out_tx.downgrade(),
                };
                // TODO: spawn task

                descriptor.state = DescriptorState::Subscribed { handle, packet_tx, frame_tx };
                // TODO: send completion
                // for result_tx in result_txs {
                //     result_tx.send(Ok(frame_rx.resubscribe()));
                // }
            }
            DescriptorState::Subscribed { handle: existing_handle, packet_tx: _, frame_tx: _ } => {
                *existing_handle = handle
            }
        }
    }

    fn handle_packet_received(&mut self, bytes: Bytes) -> Result<(), InternalError> {
        let dtp = Dtp::deserialize(bytes).context("Failed to deserialize packet")?;

        // TODO: this is O(n), use index instead
        let descriptor = self
            .descriptors
            .iter()
            .find(|(_, descriptor)| match descriptor.state {
                DescriptorState::Subscribed { handle, packet_tx: _, frame_tx: _ } => {
                    dtp.header.track_handle == handle
                }
                _ => false,
            })
            .map(|entry| entry.1);

        let Some(descriptor) = descriptor else {
            Err(anyhow!("Received packet for unknown track {}", dtp.header.track_handle))?
        };
        let DescriptorState::Subscribed { handle: _, packet_tx, frame_tx: _ } = &descriptor.state
        else {
            Err(anyhow!(
                "Received packet for track {} without subscription",
                dtp.header.track_handle
            ))?
        };
        packet_tx.send(dtp);
        Ok(())
    }

    /// Performs cleanup before the task ends.
    fn shutdown(self) {
        for (_, descriptor) in self.descriptors {
            match descriptor.state {
                DescriptorState::Available | DescriptorState::Subscribed { .. } => {}
                DescriptorState::Pending { result_txs } => {
                    for result_tx in result_txs {
                        result_tx.send(Err(SubscribeError::Disconnected));
                    }
                }
            }
            descriptor.state_tx.send(TrackState::Unpublished);
        }
    }
}
