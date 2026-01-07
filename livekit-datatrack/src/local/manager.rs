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
    /// Encoded packet is ready to be sent over the transport.
    PacketAvailable(Bytes),
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

#[derive(Debug)]
pub struct ManagerOptions {
    pub encryption: Option<Arc<dyn EncryptionProvider>>,
}

/// Manager for local data tracks.
#[derive(Debug, Clone)]
pub struct Manager {
    event_in_tx: mpsc::Sender<InputEvent>,
}

impl Manager {
    pub fn new(options: ManagerOptions) -> (Self, ManagerTask, impl Stream<Item = OutputEvent>) {
        let (event_in_tx, event_in_rx) = mpsc::channel(Self::INPUT_BUFFER_SIZE);
        let (event_out_tx, signal_out_rx) = mpsc::channel(Self::OUTPUT_BUFFER_SIZE);

        let manager = Self { event_in_tx: event_in_tx.clone() };
        let task = ManagerTask {
            encryption: options.encryption,
            event_in_tx: event_in_tx.downgrade(),
            event_in_rx,
            event_out_tx,
            handle_allocator: dtp::HandleAllocator::default(),
            descriptors: HashMap::new(),
        };

        let event_out_stream = ReceiverStream::new(signal_out_rx);
        (manager, task, event_out_stream)
    }

    /// Sends an input event to the manager's task to be processed.
    pub fn send(&self, event: InputEvent) -> Result<(), InternalError> {
        Ok(self.event_in_tx.try_send(event).context("Failed to handle input event")?)
    }

    /// Publishes a data track with the given options.
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

    /// Number of [`InputEvent`]s to buffer.
    const INPUT_BUFFER_SIZE: usize = 4;

    /// Number of [`OutputEvent`]s to buffer.
    const OUTPUT_BUFFER_SIZE: usize = 4;
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

pub struct ManagerTask {
    encryption: Option<Arc<dyn EncryptionProvider>>,
    event_in_tx: mpsc::WeakSender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,
    handle_allocator: dtp::HandleAllocator,
    descriptors: HashMap<Handle, Descriptor>,
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
        self.shutdown().await;
    }

    fn handle_event(&mut self, event: InputEvent) -> Result<(), InternalError> {
        match event {
            InputEvent::Publish(event) => self.handle_publish(event),
            InputEvent::PublishResult(event) => self.handle_publish_result(event),
            InputEvent::Unpublish(event) => self.handle_unpublished(event),
            _ => Ok(()),
        }
    }

    fn handle_publish(&mut self, event: PublishEvent) -> Result<(), InternalError> {
        let Some(handle) = self.handle_allocator.get() else {
            _ = event.result_tx.send(Err(PublishError::LimitReached));
            return Ok(());
        };

        if self.descriptors.contains_key(&handle) {
            _ = event.result_tx.send(Err(PublishError::Internal(
                anyhow!("Descriptor for handle already exists").into(),
            )));
            return Ok(());
        }
        self.descriptors.insert(handle, Descriptor::Pending(event.result_tx));

        let publish_requested = PublishRequestEvent {
            handle,
            name: event.options.name,
            uses_e2ee: self.encryption.is_some() && !event.options.disable_e2ee,
        };
        _ = self.event_out_tx.try_send(publish_requested.into()); // TODO: check for error.
        self.schedule_publish_timeout(handle);
        Ok(())
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

    fn handle_publish_result(&mut self, event: PublishResultEvent) -> Result<(), InternalError> {
        let Some(descriptor) = self.descriptors.remove(&event.handle) else {
            Err(anyhow!("No descriptor for {}", event.handle))?
        };
        let Descriptor::Pending(result_tx) = descriptor else {
            Err(anyhow!("Track {} already active", event.handle))?
        };

        if result_tx.is_closed() {
            return Ok(());
        }
        let result = event.result.map(|track_info| self.create_local_track(track_info));
        _ = result_tx.send(result);
        Ok(())
    }

    fn create_local_track(&mut self, info: DataTrackInfo) -> LocalDataTrack {
        let (frame_tx, frame_rx) = mpsc::channel(4); // TODO: tune
        let (state_tx, state_rx) = watch::channel(LocalTrackState::Published);
        let info = Arc::new(info);

        let task = LocalTrackTask {
            // TODO: handle cancellation
            packetizer: Packetizer::new(info.handle, Self::TRANSPORT_MTU),
            encryption: self.encryption.clone(),
            info: info.clone(),
            frame_rx,
            state_rx,
            event_out_tx: self.event_out_tx.clone(),
        };
        let join_handle = livekit_runtime::spawn(task.run());
        self.descriptors.insert(info.handle, Descriptor::Active { state_tx: state_tx.clone(), join_handle });

        let inner = LocalTrackInner { frame_tx, state_tx };
        LocalDataTrack::new(info, inner)
    }

    fn handle_unpublished(&mut self, event: UnpublishEvent) -> Result<(), InternalError> {
        let Some(descriptor) = self.descriptors.remove(&event.handle) else {
            Err(anyhow!("No descriptor for track {}", event.handle))?
        };
        let Descriptor::Active { state_tx, .. } = descriptor else {
            Err(anyhow!("Cannot unpublish pending track {}", event.handle))?
        };
        if !state_tx.borrow().is_published() {
            return Ok(());
        }
        state_tx
            .send(LocalTrackState::Unpublished { initiator: UnpublishInitiator::Sfu })
            .context("Failed to set state")?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dtp::Dtp;
    use futures_util::StreamExt;
    use livekit_runtime::sleep;
    use rstest::*;

    #[tokio::test]
    async fn test_task_shutdown() {
        let options = ManagerOptions { encryption: None };
        let (manager, manager_task, _) = Manager::new(options);

        let join_handle = livekit_runtime::spawn(manager_task.run());
        _ = manager.send(InputEvent::Shutdown);

        time::timeout(Duration::from_secs(1), join_handle).await.unwrap();
    }

    #[rstest]
    #[case("my_track", 10, 256)]
    #[tokio::test]
    async fn test_publish(
        #[case] name: String,
        #[case] packet_count: usize,
        #[case] payload_size: usize,
    ) {
        let options = ManagerOptions { encryption: None };
        let (manager, manager_task, mut output_events) = Manager::new(options);
        livekit_runtime::spawn(manager_task.run());

        let handle_events = async {
            let mut packets_sent = 0;
            while let Some(event) = output_events.next().await {
                match event {
                    OutputEvent::PublishRequest(event) => {
                        // SFU accepts publication
                        let info = DataTrackInfo {
                            sid: "DTR_1234".into(),
                            handle: 1u32.try_into().unwrap(),
                            name: event.name,
                            uses_e2ee: event.uses_e2ee,
                        };
                        let input_event =
                            PublishResultEvent { handle: event.handle, result: Ok(info) };
                        _ = manager.send(input_event.into());
                    }
                    OutputEvent::PacketAvailable(packet) => {
                        let payload = Dtp::deserialize(packet).unwrap().payload;
                        assert_eq!(payload.len(), payload_size);
                        packets_sent += 1;
                    }
                    OutputEvent::UnpublishRequest(event) => {
                        assert_eq!(event.handle, 1u32.try_into().unwrap());
                        assert_eq!(packets_sent, packet_count);
                        break;
                    }
                }
            }
        };
        let publish_track = async {
            let track_options = DataTrackOptions::with_name(name.clone());
            let track = manager.publish_track(track_options).await.unwrap();
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
