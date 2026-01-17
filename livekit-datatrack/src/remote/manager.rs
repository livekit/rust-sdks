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

use super::{depacketizer::Depacketizer, RemoteDataTrack, RemoteTrackInner};
use crate::{
    api::{DataTrackFrame, DataTrackInfo, DataTrackSid, InternalError, SubscribeError},
    dtp::{Dtp, Handle},
    e2ee::DecryptionProvider,
    remote::pipeline::RemoteTrackTask,
    utils::HandleMap,
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use from_variants::FromVariants;
use std::{
    collections::{HashMap, HashSet},
    mem,
    sync::Arc,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_stream::{wrappers::ReceiverStream, Stream};

/// An external event handled by [`Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    PublicationUpdates(PublicationUpdatesEvent),
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

/// Subscriber handles available or updated.
#[derive(Debug)]
pub struct SubscriberHandlesEvent {
    /// Mapping between track handles attached to incoming packets to the
    /// track SIDs they belong to.
    pub mapping: HashMap<Handle, DataTrackSid>,
}

type SubscribeResult = Result<broadcast::Receiver<DataTrackFrame>, SubscribeError>;

/// User requested to subscribe to a track.
#[derive(Debug)]
pub struct SubscribeEvent {
    /// Identifier of the track.
    pub(super) sid: DataTrackSid,
    /// Async completion channel.
    pub(super) result_tx: oneshot::Sender<SubscribeResult>,
}

/// User subscribed or unsubscribed to a track.
#[derive(Debug)]
pub struct SubscriptionUpdatedEvent {
    /// Identifier of the affected track.
    pub sid: DataTrackSid,
    /// Whether to subscribe or unsubscribe.
    pub subscribe: bool,
}

/// Options for creating a [`Manager`].
#[derive(Debug)]
pub struct ManagerOptions {
    /// Provider to use for decrypting incoming frame payloads.
    ///
    /// If none, remote tracks using end-to-end encryption will not be available
    /// for subscription.
    ///
    pub decryption: Option<Arc<dyn DecryptionProvider>>,
}

/// System for managing data track subscriptions.
pub struct Manager {
    decryption: Option<Arc<dyn DecryptionProvider>>,
    event_in_tx: mpsc::WeakSender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,

    /// Mapping between track SID and descriptor.
    descriptors: HashMap<DataTrackSid, Descriptor>,
    /// Bidirectional mapping between track SID and subscriber handle.
    sub_handles: HandleMap,
}

impl Manager {
    /// Creates a new manager.
    ///
    /// Returns a tuple containing the following:
    ///
    /// - The manager itself to be spawned by the caller (see [`Manager::run`]).
    /// - Channel for sending [`InputEvent`]s to be processed by the manager.
    /// - Stream for receiving [`OutputEvent`]s produced by the manager.
    ///
    pub fn new(options: ManagerOptions) -> (Self, ManagerInput, impl Stream<Item = OutputEvent>) {
        let (event_in_tx, event_in_rx) = mpsc::channel(4); // TODO: tune buffer size
        let (event_out_tx, event_out_rx) = mpsc::channel(4);

        let event_in = ManagerInput { event_in_tx: event_in_tx.clone() };
        let manager = Manager {
            decryption: options.decryption,
            event_in_tx: event_in_tx.downgrade(),
            event_in_rx,
            event_out_tx,
            descriptors: HashMap::default(),
            sub_handles: HandleMap::default(),
        };

        let event_out = ReceiverStream::new(event_out_rx);
        (manager, event_in, event_out)
    }

    /// Run the manager task, consuming self.
    ///
    /// The manager will continue running until receiving [`InputEvent::Shutdown`].
    ///
    pub async fn run(mut self) {
        log::debug!("Task started");
        while let Some(event) = self.event_in_rx.recv().await {
            log::debug!("Input event: {:?}", event);
            match event {
                InputEvent::PublicationUpdates(event) => {
                    self.handle_publication_updates(event).await
                }
                InputEvent::Subscribe(event) => self.handle_subscribe(event).await,
                InputEvent::SubscriberHandles(event) => self.handle_subscriber_handles(event),
                InputEvent::PacketReceived(bytes) => self.handle_packet_received(bytes),
                InputEvent::Shutdown => break,
            }
        }
        self.shutdown().await;
        log::debug!("Task ended");
    }

    async fn handle_publication_updates(&mut self, event: PublicationUpdatesEvent) {
        if event.updates.is_empty() {
            return;
        }
        let mut sids_in_update = HashSet::new();

        // Detect published track
        for (publisher_identity, tracks) in event.updates {
            for info in tracks {
                sids_in_update.insert(info.sid.clone());
                if self.descriptors.contains_key(&info.sid) {
                    continue;
                }
                self.handle_track_published(publisher_identity.clone(), info).await;
            }
        }

        // Detect unpublished tracks
        let unpublished_sids: Vec<_> =
            self.descriptors.keys().filter(|sid| !sids_in_update.contains(*sid)).cloned().collect();
        for sid in unpublished_sids {
            self.handle_track_unpublished(sid.clone());
        }
    }

    async fn handle_track_published(&mut self, publisher_identity: String, info: DataTrackInfo) {
        log::debug!("Track published: sid={}", info.sid);
        if self.descriptors.contains_key(&info.sid) {
            log::error!("Existing descriptor for track {}", info.sid);
            return;
        }
        let info = Arc::new(info);
        let publisher_identity: Arc<str> = publisher_identity.into();

        let (state_tx, state_rx) = watch::channel(TrackState::Published);
        let descriptor = Descriptor {
            info: info.clone(),
            publisher_identity: publisher_identity.clone(),
            state_tx,
            state: DescriptorState::Available,
        };
        self.descriptors.insert(descriptor.info.sid.clone(), descriptor);

        let inner = RemoteTrackInner {
            state_rx,
            event_in_tx: self.event_in_tx.clone(),
            publisher_identity,
        };
        let track = RemoteDataTrack::new(info, inner);
        _ = self.event_out_tx.send(track.into()).await;
    }

    fn handle_track_unpublished(&mut self, sid: DataTrackSid) {
        log::debug!("Track unpublished: sid={}", sid);
        self.sub_handles.remove(&sid);
        let Some(descriptor) = self.descriptors.remove(&sid) else {
            log::error!("Unknown track {}", sid);
            return;
        };
        _ = descriptor.state_tx.send(TrackState::Unpublished);
        // TODO: this should end the track task
    }

    async fn handle_subscribe(&mut self, event: SubscribeEvent) {
        let Some(descriptor) = self.descriptors.get_mut(&event.sid) else {
            let error =
                SubscribeError::Internal(anyhow!("Cannot subscribe to unknown track").into());
            _ = event.result_tx.send(Err(error));
            return;
        };
        match &mut descriptor.state {
            DescriptorState::Available => {
                let update_event =
                    SubscriptionUpdatedEvent { sid: event.sid.clone(), subscribe: true };
                _ = self.event_out_tx.send(update_event.into()).await;
                descriptor.state =
                    DescriptorState::PendingSubscriberHandle { result_txs: vec![event.result_tx] };
                // TODO: schedule timeout internally
            }
            DescriptorState::PendingSubscriberHandle { result_txs } => {
                result_txs.push(event.result_tx);
            }
            DescriptorState::Subscribed { frame_tx, .. } => {
                let frame_rx = frame_tx.subscribe();
                _ = event.result_tx.send(Ok(frame_rx))
            }
        }
    }

    fn handle_subscriber_handles(&mut self, event: SubscriberHandlesEvent) {
        for (handle, sid) in event.mapping {
            self.register_subscriber_handle(handle, sid);
        }
    }

    fn register_subscriber_handle(&mut self, handle: Handle, sid: DataTrackSid) {
        let Some(descriptor) = self.descriptors.get_mut(&sid) else {
            log::warn!("Unknown track: {}", sid);
            return;
        };
        let result_txs = match &mut descriptor.state {
            DescriptorState::Available => {
                log::warn!("No subscription");
                return;
            }
            DescriptorState::Subscribed { .. } => {
                log::warn!("Handle reassignment not implemented");
                return;
            }
            DescriptorState::PendingSubscriberHandle { result_txs } => mem::take(result_txs),
        };

        let (packet_tx, packet_rx) = mpsc::channel(4); // TODO: tune
        let (frame_tx, frame_rx) = broadcast::channel(4);

        let track_task = RemoteTrackTask {
            depacketizer: Depacketizer::new(),
            decryption: self.decryption.clone(),
            info: descriptor.info.clone(),
            publisher_identity: descriptor.publisher_identity.clone(),
            state_rx: descriptor.state_tx.subscribe(),
            packet_rx,
            frame_tx: frame_tx.clone(),
            event_out_tx: self.event_out_tx.downgrade(),
        };
        let join_handle = livekit_runtime::spawn(track_task.run());

        descriptor.state = DescriptorState::Subscribed { packet_tx, frame_tx, join_handle };
        self.sub_handles.insert(handle, sid);

        for result_tx in result_txs {
            _ = result_tx.send(Ok(frame_rx.resubscribe()));
        }
    }

    fn handle_packet_received(&mut self, bytes: Bytes) {
        let dtp = match Dtp::deserialize(bytes) {
            Ok(dtp) => dtp,
            Err(err) => {
                log::error!("Failed to deserialize DTP: {}", err);
                return;
            }
        };
        let Some(sid) = self.sub_handles.get_sid(dtp.header.track_handle) else {
            log::warn!("Unknown subscriber handle {}", dtp.header.track_handle);
            return;
        };
        let Some(descriptor) = self.descriptors.get(sid) else {
            log::warn!("Missing descriptor");
            return;
        };
        let DescriptorState::Subscribed { packet_tx, .. } = &descriptor.state else {
            log::warn!("Received packet for track {} without subscription", descriptor.info.sid);
            return;
        };
        _ = packet_tx
            .try_send(dtp)
            .inspect_err(|err| log::debug!("Cannot send packet to track task: {}", err));
    }

    /// Performs cleanup before the task ends.
    async fn shutdown(self) {
        for (_, descriptor) in self.descriptors {
            _ = descriptor.state_tx.send(TrackState::Unpublished);
            match descriptor.state {
                DescriptorState::Available { .. } => {}
                DescriptorState::PendingSubscriberHandle { result_txs } => {
                    for result_tx in result_txs {
                        _ = result_tx.send(Err(SubscribeError::Disconnected));
                    }
                }
                DescriptorState::Subscribed { join_handle, .. } => join_handle.await,
            }
        }
    }
}

#[derive(Debug)]
struct Descriptor {
    info: Arc<DataTrackInfo>,
    publisher_identity: Arc<str>,
    state_tx: watch::Sender<TrackState>,
    state: DescriptorState,
}

#[derive(Debug)]
enum DescriptorState {
    Available,
    PendingSubscriberHandle {
        result_txs: Vec<oneshot::Sender<SubscribeResult>>,
    },
    Subscribed {
        packet_tx: mpsc::Sender<Dtp>,
        frame_tx: broadcast::Sender<DataTrackFrame>,
        join_handle: livekit_runtime::JoinHandle<()>,
    },
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

/// Channel for sending [`InputEvent`]s to [`Manager`].
#[derive(Debug, Clone)]
pub struct ManagerInput {
    event_in_tx: mpsc::Sender<InputEvent>,
}

impl ManagerInput {
    /// Sends an input event to the manager's task to be processed.
    pub fn send(&self, event: InputEvent) -> Result<(), InternalError> {
        Ok(self.event_in_tx.try_send(event).context("Failed to send input event")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake::{
        faker::{internet::en::SafeEmail, lorem::en::Word},
        Fake, Faker,
    };
    use futures_util::StreamExt;
    use std::{collections::HashMap, time::Duration};
    use tokio::time;

    #[tokio::test]
    async fn test_task_shutdown() {
        let options = ManagerOptions { decryption: None };
        let (manager, input, _) = Manager::new(options);

        let join_handle = livekit_runtime::spawn(manager.run());
        _ = input.send(InputEvent::Shutdown);

        time::timeout(Duration::from_secs(1), join_handle).await.unwrap();
    }

    #[tokio::test]
    async fn test_subscribe() {
        let publisher_identity: String = SafeEmail().fake();
        let track_name: String = Word().fake();
        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();

        let options = ManagerOptions { decryption: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        // Simulate track published
        let event = PublicationUpdatesEvent {
            updates: HashMap::from([(
                publisher_identity.clone(),
                vec![DataTrackInfo {
                    sid: track_sid.clone(),
                    pub_handle: Faker.fake(), // Pub handle
                    name: track_name.clone(),
                    uses_e2ee: false,
                }],
            )]),
        };
        _ = input.send(event.into());

        let wait_for_track = async {
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::TrackAvailable(track) => return track,
                    _ => continue,
                }
            }
            panic!("No track received");
        };

        let track = wait_for_track.await;
        assert!(track.is_published());
        assert_eq!(track.info().name, track_name);
        assert_eq!(*track.info().sid(), track_sid);
        assert_eq!(track.publisher_identity(), publisher_identity);

        let simulate_subscriber_handles = async {
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::SubscriptionUpdated(event) => {
                        assert!(event.subscribe);
                        assert_eq!(event.sid, track_sid);
                        time::sleep(Duration::from_millis(20)).await;

                        // Simulate SFU reply
                        let event = SubscriberHandlesEvent {
                            mapping: HashMap::from([(sub_handle, track_sid.clone())]),
                        };
                        _ = input.send(event.into());
                    }
                    _ => {}
                }
            }
        };

        time::timeout(Duration::from_secs(1), async {
            tokio::select! {
                _ = simulate_subscriber_handles => {}
                _ = track.subscribe() => {}
            }
        })
        .await
        .unwrap();
    }
}
