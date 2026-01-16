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

use super::{packetizer::Packetizer, pipeline::LocalTrackTask, LocalTrackInner};
use crate::{
    api::{DataTrackInfo, DataTrackOptions, InternalError, PublishError},
    dtp::{self, Handle},
    e2ee::EncryptionProvider,
    local::LocalDataTrack,
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use from_variants::FromVariants;
use futures_core::Stream;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time;
use tokio_stream::wrappers::ReceiverStream;

/// An external event handled by [`Manager`].
#[derive(Debug, FromVariants)]
pub enum InputEvent {
    Publish(PublishEvent),
    PublishResult(PublishResultEvent),
    Unpublish(UnpublishEvent),
    /// Shutdown the manager and all associated tracks.
    Shutdown,
}

/// An event produced by [`Manager`] requiring external action.
#[derive(Debug, FromVariants)]
pub enum OutputEvent {
    PublishRequest(PublishRequestEvent),
    UnpublishRequest(UnpublishRequestEvent),
    /// Serialized packets are ready to be sent over the transport.
    PacketsAvailable(Vec<Bytes>),
}

/// Result of a publish request.
#[derive(Debug)]
pub struct PublishResultEvent {
    /// Publisher handle of the track.
    pub handle: Handle,
    /// Outcome of the publish request.
    pub result: Result<DataTrackInfo, PublishError>,
}

/// SFU notification that a track published by the local participant
/// has been unpublished.
#[derive(Debug)]
pub struct UnpublishEvent {
    /// Publisher handle of the track that was unpublished.
    pub handle: Handle,
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

/// Request to publish a data track.
#[derive(Debug)]
pub struct PublishEvent {
    /// Publish options.
    options: DataTrackOptions,
    /// Async completion channel.
    result_tx: oneshot::Sender<Result<LocalDataTrack, PublishError>>,
}

/// Request to publish a data track timed-out.
#[derive(Debug)]
pub struct PublishTimeoutEvent {
    /// Publisher handle of the pending publication.
    handle: Handle,
}

/// Options for creating a [`Manager`].
#[derive(Debug)]
pub struct ManagerOptions {
    /// Provider to use for encrypting outgoing frame payloads.
    ///
    /// If none, end-to-end encryption will be disabled for all published tracks.
    ///
    pub encryption: Option<Arc<dyn EncryptionProvider>>,
}

/// System for managing data track publications.
pub struct Manager {
    encryption: Option<Arc<dyn EncryptionProvider>>,
    event_in_tx: mpsc::WeakSender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,
    handle_allocator: dtp::HandleAllocator,
    descriptors: HashMap<Handle, Descriptor>,
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
            encryption: options.encryption,
            event_in_tx: event_in_tx.downgrade(),
            event_in_rx,
            event_out_tx,
            handle_allocator: dtp::HandleAllocator::default(),
            descriptors: HashMap::new(),
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
                InputEvent::Publish(event) => self.handle_publish(event),
                InputEvent::PublishResult(event) => self.handle_publish_result(event),
                InputEvent::Unpublish(event) => self.handle_unpublished(event),
                InputEvent::Shutdown => break,
            }
        }
        self.shutdown().await;
        log::debug!("Task ended");
    }

    fn handle_publish(&mut self, event: PublishEvent) {
        let Some(handle) = self.handle_allocator.get() else {
            _ = event.result_tx.send(Err(PublishError::LimitReached));
            return;
        };

        if self.descriptors.contains_key(&handle) {
            _ = event.result_tx.send(Err(PublishError::Internal(
                anyhow!("Descriptor for handle already exists").into(),
            )));
            return;
        }
        self.descriptors.insert(handle, Descriptor::Pending(event.result_tx));

        let publish_requested = PublishRequestEvent {
            handle,
            name: event.options.name,
            uses_e2ee: self.encryption.is_some() && !event.options.disable_e2ee,
        };
        _ = self.event_out_tx.try_send(publish_requested.into()); // TODO: check for error.
        self.schedule_publish_timeout(handle);
    }

    fn schedule_publish_timeout(&self, handle: Handle) {
        let event_in_tx = self.event_in_tx.clone();
        let emit_timeout = async move {
            time::sleep(Self::PUBLISH_TIMEOUT).await;
            let Some(tx) = event_in_tx.upgrade() else { return };
            let event = PublishResultEvent { handle, result: Err(PublishError::Timeout) };
            _ = tx.try_send(event.into())
        };
        livekit_runtime::spawn(emit_timeout);
    }

    fn handle_publish_result(&mut self, event: PublishResultEvent) {
        let Some(descriptor) = self.descriptors.remove(&event.handle) else {
            log::warn!("No descriptor for {}", event.handle);
            return
        };
        let Descriptor::Pending(result_tx) = descriptor else {
            log::warn!("Track {} already active", event.handle);
            return
        };

        if result_tx.is_closed() {
            return;
        }
        let result = event.result.map(|track_info| self.create_local_track(track_info));
        _ = result_tx.send(result);
    }

    fn create_local_track(&mut self, info: DataTrackInfo) -> LocalDataTrack {
        let (frame_tx, frame_rx) = mpsc::channel(4); // TODO: tune
        let (state_tx, state_rx) = watch::channel(LocalTrackState::Published);
        let info = Arc::new(info);

        let task = LocalTrackTask {
            // TODO: handle cancellation
            packetizer: Packetizer::new(info.pub_handle, Self::TRANSPORT_MTU),
            encryption: self.encryption.clone(),
            info: info.clone(),
            frame_rx,
            state_rx,
            event_out_tx: self.event_out_tx.clone(),
        };
        let join_handle = livekit_runtime::spawn(task.run());
        self.descriptors.insert(info.pub_handle, Descriptor::Active { state_tx: state_tx.clone(), join_handle });

        let inner = LocalTrackInner { frame_tx, state_tx };
        LocalDataTrack::new(info, inner)
    }

    fn handle_unpublished(&mut self, event: UnpublishEvent) {
        let Some(descriptor) = self.descriptors.remove(&event.handle) else {
            log::warn!("No descriptor for track {}", event.handle);
            return
        };
        let Descriptor::Active { state_tx, .. } = descriptor else {
            log::warn!("Cannot unpublish pending track {}", event.handle);
            return
        };
        if !state_tx.borrow().is_published() {
            return
        }
        _ = state_tx
            .send(LocalTrackState::Unpublished { initiator: UnpublishInitiator::Sfu });
    }

    /// Performs cleanup before the task ends.
    async fn shutdown(self) {
        for (_, descriptor) in self.descriptors {
            match descriptor {
                Descriptor::Pending(result_tx) => {
                    _ = result_tx.send(Err(PublishError::Disconnected))
                }
                Descriptor::Active { state_tx, join_handle } => {
                    _ = state_tx.send(LocalTrackState::Unpublished {
                        initiator: UnpublishInitiator::Shutdown,
                    });
                    join_handle.await;
                }
            }
        }
    }

    /// How long to wait for an SFU response for a track publication before timeout.
    const PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);

    /// MTU of the transport
    const TRANSPORT_MTU: usize = 16_000;
}

#[derive(Debug)]
enum Descriptor {
    /// Publication is awaiting SFU response.
    ///
    /// The associated channel is used to send a result to the user,
    /// either the local track or a publish error.
    ///
    Pending(oneshot::Sender<Result<LocalDataTrack, PublishError>>),
    /// Publication is active.
    ///
    /// The associated channel is used to send state updates to the track's task.
    ///
    Active {
        state_tx: watch::Sender<LocalTrackState>,
        join_handle: livekit_runtime::JoinHandle<()>
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum UnpublishInitiator {
    Client,
    Sfu,
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LocalTrackState {
    Published,
    Unpublished { initiator: UnpublishInitiator },
}

impl LocalTrackState {
    pub fn is_published(&self) -> bool {
        matches!(self, Self::Published)
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
        Ok(self.event_in_tx.try_send(event).context("Failed to handle input event")?)
    }

    /// Publishes a data track with given options.
    pub async fn publish_track(
        &self,
        options: DataTrackOptions,
    ) -> Result<LocalDataTrack, PublishError> {
        let (result_tx, result_rx) = oneshot::channel();
        let event = PublishEvent { options, result_tx };

        self.event_in_tx.try_send(event.into()).map_err(|_| PublishError::Disconnected)?;
        let track = result_rx.await.map_err(|_| PublishError::Disconnected)??;

        Ok(track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::DataTrackSid, dtp::Dtp};
    use futures_util::StreamExt;
    use livekit_runtime::sleep;
    use fake::{Fake, Faker, faker::lorem::en::Word};

    #[tokio::test]
    async fn test_task_shutdown() {
        let options = ManagerOptions { encryption: None };
        let (manager, input, _) = Manager::new(options);

        let join_handle = livekit_runtime::spawn(manager.run());
        _ = input.send(InputEvent::Shutdown);

        time::timeout(Duration::from_secs(1), join_handle).await.unwrap();
    }

    #[tokio::test]
    async fn test_publish() {
        let payload_size = 256;
        let packet_count = 10;

        let track_name: String = Word().fake();
        let track_sid: DataTrackSid = Faker.fake();
        let pub_handle: Handle = Faker.fake();

        let options = ManagerOptions { encryption: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_name_clone = track_name.clone();
        let handle_events = async {
            let mut packets_sent = 0;
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::PublishRequest(event) => {
                        assert!(!event.uses_e2ee);
                        assert_eq!(event.name, track_name_clone);

                        // SFU accepts publication
                        let info = DataTrackInfo {
                            sid: track_sid.clone(),
                            pub_handle,
                            name: event.name,
                            uses_e2ee: event.uses_e2ee,
                        };
                        let input_event =
                            PublishResultEvent { handle: event.handle, result: Ok(info) };
                        _ = input.send(input_event.into());
                    }
                    OutputEvent::PacketsAvailable(packets) => {
                        let packet = packets.into_iter().nth(0).unwrap();
                        let payload = Dtp::deserialize(packet).unwrap().payload;
                        assert_eq!(payload.len(), payload_size);
                        packets_sent += 1;
                    }
                    OutputEvent::UnpublishRequest(event) => {
                        assert_eq!(event.handle, pub_handle);
                        assert_eq!(packets_sent, packet_count);
                        break;
                    }
                }
            }
        };
        let publish_track = async {
            let track_options = DataTrackOptions::with_name(track_name.clone());
            let track = input.publish_track(track_options).await.unwrap();
            assert!(!track.info().uses_e2ee());
            assert_eq!(track.info().name(), track_name);
            assert_eq!(*track.info().sid(), track_sid);

            for _ in 0..packet_count {
                track.publish(vec![0xFA; payload_size].into()).unwrap();
                sleep(Duration::from_millis(10)).await;
            }
            // Only reference to track dropped here (unpublish)
        };
        time::timeout(Duration::from_secs(1), async { tokio::join!(publish_track, handle_events) })
            .await
            .unwrap();
    }
}
