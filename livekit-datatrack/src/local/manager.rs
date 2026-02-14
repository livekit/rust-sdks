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

use super::{
    events::*,
    pipeline::{Pipeline, PipelineOptions},
    LocalTrackInner,
};
use crate::{
    api::{DataTrackFrame, DataTrackInfo, DataTrackOptions, InternalError, PublishError},
    e2ee::EncryptionProvider,
    local::LocalDataTrack,
    packet::{self, Handle},
};
use anyhow::{anyhow, Context};
use futures_core::Stream;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_stream::wrappers::ReceiverStream;

/// Options for creating a [`Manager`].
#[derive(Debug)]
pub struct ManagerOptions {
    /// Provider to use for encrypting outgoing frame payloads.
    ///
    /// If none, end-to-end encryption will be disabled for all published tracks.
    ///
    pub encryption_provider: Option<Arc<dyn EncryptionProvider>>,
}

/// System for managing data track publications.
pub struct Manager {
    encryption_provider: Option<Arc<dyn EncryptionProvider>>,
    event_in_tx: mpsc::Sender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,
    handle_allocator: packet::HandleAllocator,
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

        let event_in = ManagerInput::new(event_in_tx.clone());
        let manager = Manager {
            encryption_provider: options.encryption_provider,
            event_in_tx,
            event_in_rx,
            event_out_tx,
            handle_allocator: packet::HandleAllocator::default(),
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
                InputEvent::PublishRequest(event) => self.on_publish_request(event).await,
                InputEvent::PublishCancelled(event) => self.on_publish_cancelled(event).await,
                InputEvent::QueryPublished(event) => self.on_query_published(event).await,
                InputEvent::UnpublishRequest(event) => self.on_unpublish_request(event).await,
                InputEvent::SfuPublishResponse(event) => self.on_sfu_publish_response(event).await,
                InputEvent::SfuUnpublishResponse(event) => {
                    self.on_sfu_unpublish_response(event).await
                }
                InputEvent::RepublishTracks => self.on_republish_tracks().await,
                InputEvent::Shutdown => break,
            }
        }
        self.shutdown().await;
        log::debug!("Task ended");
    }

    async fn on_publish_request(&mut self, event: PublishRequest) {
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

        let (result_tx, result_rx) = oneshot::channel();
        self.descriptors.insert(handle, Descriptor::Pending(result_tx));

        livekit_runtime::spawn(Self::forward_publish_result(
            handle,
            result_rx,
            event.result_tx,
            self.event_in_tx.downgrade(),
        ));

        let event = SfuPublishRequest {
            handle,
            name: event.options.name,
            uses_e2ee: self.encryption_provider.is_some(),
        };
        _ = self.event_out_tx.send(event.into()).await;
    }

    /// Task that awaits a pending publish result.
    ///
    /// Forwards the result to the user, or notifies the manager if the receiver
    /// is dropped (e.g., due to timeout) so it can remove the pending publication.
    ///
    async fn forward_publish_result(
        handle: Handle,
        result_rx: oneshot::Receiver<Result<LocalDataTrack, PublishError>>,
        mut forward_result_tx: oneshot::Sender<Result<LocalDataTrack, PublishError>>,
        event_in_tx: mpsc::WeakSender<InputEvent>,
    ) {
        tokio::select! {
            biased;
            Ok(result) = result_rx => {
                _ = forward_result_tx.send(result);
            }
            _ = forward_result_tx.closed() => {
                let Some(tx) = event_in_tx.upgrade() else { return };
                let event = PublishCancelled { handle };
                _ = tx.try_send(event.into());
            }
        }
    }

    async fn on_publish_cancelled(&mut self, event: PublishCancelled) {
        if self.descriptors.remove(&event.handle).is_none() {
            log::warn!("No descriptor for {}", event.handle);
        }
    }

    async fn on_query_published(&self, event: QueryPublished) {
        let published_info: Vec<_> = self
            .descriptors
            .iter()
            .filter_map(|descriptor| {
                let (_, Descriptor::Active { info, .. }) = descriptor else {
                    return None;
                };
                info.clone().into()
            })
            .collect();
        _ = event.result_tx.send(published_info);
    }

    async fn on_unpublish_request(&mut self, event: UnpublishRequest) {
        self.remove_descriptor(event.handle);

        let event = SfuUnpublishRequest { handle: event.handle };
        _ = self.event_out_tx.send(event.into()).await;
    }

    async fn on_sfu_publish_response(&mut self, event: SfuPublishResponse) {
        let Some(descriptor) = self.descriptors.remove(&event.handle) else {
            // This can occur if a publish request is cancelled before the SFU responds,
            // send an unpublish request to ensure consistent SFU state.
            _ = self.event_out_tx.send(SfuUnpublishRequest { handle: event.handle }.into()).await;
            return;
        };
        match descriptor {
            Descriptor::Pending(result_tx) => {
                // SFU accepted initial publication request
                if result_tx.is_closed() {
                    return;
                }
                let result = event.result.map(|track_info| self.create_local_track(track_info));
                _ = result_tx.send(result);
                return;
            }
            Descriptor::Active { ref state_tx, ref info, .. } => {
                if *state_tx.borrow() != PublishState::Republishing {
                    log::warn!("Track {} already active", event.handle);
                    return;
                }
                let Ok(updated_info) = event.result else {
                    log::warn!("Republish failed for track {}", event.handle);
                    return;
                };

                log::debug!("Track {} republished", event.handle);
                {
                    let mut sid = info.sid.write().unwrap();
                    *sid = updated_info.sid();
                }
                _ = state_tx.send(PublishState::Published);
                self.descriptors.insert(event.handle, descriptor);
            }
        }
    }

    fn create_local_track(&mut self, info: DataTrackInfo) -> LocalDataTrack {
        let info = Arc::new(info);
        let encryption_provider =
            if info.uses_e2ee() { self.encryption_provider.as_ref().map(Arc::clone) } else { None };

        let pipeline_opts = PipelineOptions { info: info.clone(), encryption_provider };
        let pipeline = Pipeline::new(pipeline_opts);

        let (frame_tx, frame_rx) = mpsc::channel(4); // TODO: tune
        let (state_tx, state_rx) = watch::channel(PublishState::Published);

        let track_task = TrackTask {
            info: info.clone(),
            pipeline,
            state_rx,
            frame_rx,
            event_in_tx: self.event_in_tx.clone(),
            event_out_tx: self.event_out_tx.clone(),
        };
        let task_handle = livekit_runtime::spawn(track_task.run());

        self.descriptors.insert(
            info.pub_handle,
            Descriptor::Active { info: info.clone(), state_tx: state_tx.clone(), task_handle },
        );

        let inner = LocalTrackInner { frame_tx, state_tx };
        LocalDataTrack::new(info, inner)
    }

    async fn on_sfu_unpublish_response(&mut self, event: SfuUnpublishResponse) {
        self.remove_descriptor(event.handle);
    }

    fn remove_descriptor(&mut self, handle: Handle) {
        let Some(descriptor) = self.descriptors.remove(&handle) else {
            return;
        };
        let Descriptor::Active { state_tx, .. } = descriptor else {
            return;
        };
        if *state_tx.borrow() != PublishState::Unpublished {
            _ = state_tx.send(PublishState::Unpublished);
        }
    }

    async fn on_republish_tracks(&mut self) {
        let descriptors = std::mem::take(&mut self.descriptors);
        for (handle, descriptor) in descriptors {
            match descriptor {
                Descriptor::Pending(result_tx) => {
                    // TODO: support republish for pending publications
                    _ = result_tx.send(Err(PublishError::Disconnected));
                }
                Descriptor::Active { ref info, ref state_tx, .. } => {
                    let event = SfuPublishRequest {
                        handle: info.pub_handle,
                        name: info.name.clone(),
                        uses_e2ee: info.uses_e2ee,
                    };
                    _ = state_tx.send(PublishState::Republishing);
                    _ = self.event_out_tx.send(event.into()).await;
                    self.descriptors.insert(handle, descriptor);
                }
            }
        }
    }

    /// Performs cleanup before the task ends.
    async fn shutdown(self) {
        for (_, descriptor) in self.descriptors {
            match descriptor {
                Descriptor::Pending(result_tx) => {
                    _ = result_tx.send(Err(PublishError::Disconnected))
                }
                Descriptor::Active { state_tx, task_handle, .. } => {
                    _ = state_tx.send(PublishState::Unpublished);
                    task_handle.await;
                }
            }
        }
    }
}

/// Task for an individual published data track.
struct TrackTask {
    info: Arc<DataTrackInfo>,
    pipeline: Pipeline,
    state_rx: watch::Receiver<PublishState>,
    frame_rx: mpsc::Receiver<DataTrackFrame>,
    event_in_tx: mpsc::Sender<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,
}

impl TrackTask {
    async fn run(mut self) {
        let sid = self.info.sid();
        log::debug!("Track task started: sid={}", sid);

        let mut state = *self.state_rx.borrow();
        while state != PublishState::Unpublished {
            tokio::select! {
                _ = self.state_rx.changed() => {
                    state = *self.state_rx.borrow();
                }
                Some(frame) = self.frame_rx.recv() => {
                    if state == PublishState::Republishing {
                        // Drop frames while republishing.
                        continue;
                    }
                    self.process_and_send(frame);
                }
            }
        }

        let event = UnpublishRequest { handle: self.info.pub_handle };
        _ = self.event_in_tx.send(event.into()).await;

        log::debug!("Track task ended: sid={}", sid);
    }

    fn process_and_send(&mut self, frame: DataTrackFrame) {
        let Ok(packets) = self
            .pipeline
            .process_frame(frame)
            .inspect_err(|err| log::debug!("Process failed: {}", err))
        else {
            return;
        };
        let packets: Vec<_> = packets.into_iter().map(|packet| packet.serialize()).collect();
        _ = self
            .event_out_tx
            .try_send(packets.into())
            .inspect_err(|err| log::debug!("Cannot send packets to transport: {}", err));
    }
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
    /// The associated channel is used to end the track task.
    ///
    Active {
        info: Arc<DataTrackInfo>,
        state_tx: watch::Sender<PublishState>,
        task_handle: livekit_runtime::JoinHandle<()>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishState {
    /// Track is published.
    Published,
    /// Track is being republished.
    Republishing,
    /// Track is no longer published.
    Unpublished,
}

/// Channel for sending [`InputEvent`]s to [`Manager`].
#[derive(Debug, Clone)]
pub struct ManagerInput {
    event_in_tx: mpsc::Sender<InputEvent>,
    _drop_guard: Arc<DropGuard>,
}

/// Guard that sends shutdown event when the last reference is dropped.
#[derive(Debug)]
struct DropGuard {
    event_in_tx: mpsc::Sender<InputEvent>,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        _ = self.event_in_tx.try_send(InputEvent::Shutdown);
    }
}

impl ManagerInput {
    fn new(event_in_tx: mpsc::Sender<InputEvent>) -> Self {
        Self { event_in_tx: event_in_tx.clone(), _drop_guard: DropGuard { event_in_tx }.into() }
    }

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

        let event = PublishRequest { options, result_tx };
        self.event_in_tx.try_send(event.into()).map_err(|_| PublishError::Disconnected)?;

        let track = tokio::time::timeout(Self::PUBLISH_TIMEOUT, result_rx)
            .await
            .map_err(|_| PublishError::Timeout)?
            .map_err(|_| PublishError::Disconnected)??;

        Ok(track)
    }

    /// Get information about all currently published tracks.
    ///
    /// This does not include publications that are still pending.
    ///
    pub async fn query_tracks(&self) -> Vec<Arc<DataTrackInfo>> {
        let (result_tx, result_rx) = oneshot::channel();

        let event = QueryPublished { result_tx };
        if self.event_in_tx.send(event.into()).await.is_err() {
            return vec![];
        }

        result_rx.await.unwrap_or_default()
    }

    /// How long to wait for before timeout.
    const PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use super::*;
    use crate::{api::DataTrackSid, packet::Packet};
    use fake::{Fake, Faker};
    use futures_util::StreamExt;
    use livekit_runtime::{sleep, timeout};

    #[tokio::test]
    async fn test_task_shutdown() {
        let options = ManagerOptions { encryption_provider: None };
        let (manager, input, _) = Manager::new(options);

        let join_handle = livekit_runtime::spawn(manager.run());
        _ = input.send(InputEvent::Shutdown);

        timeout(Duration::from_secs(1), join_handle).await.unwrap();
    }

    #[tokio::test]
    async fn test_publish() {
        let payload_size = 256;
        let packet_count = 10;

        let track_name: String = Faker.fake();
        let track_sid: DataTrackSid = Faker.fake();
        let pub_handle: Handle = Faker.fake();

        let options = ManagerOptions { encryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_name_clone = track_name.clone();
        let handle_events = async {
            let mut packets_sent = 0;
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::SfuPublishRequest(event) => {
                        assert!(!event.uses_e2ee);
                        assert_eq!(event.name, track_name_clone);

                        // SFU accepts publication
                        let info = DataTrackInfo {
                            sid: RwLock::new(track_sid.clone()).into(),
                            pub_handle,
                            name: event.name,
                            uses_e2ee: event.uses_e2ee,
                        };
                        let event = SfuPublishResponse { handle: event.handle, result: Ok(info) };
                        _ = input.send(event.into());
                    }
                    OutputEvent::PacketsAvailable(packets) => {
                        let packet = packets.into_iter().nth(0).unwrap();
                        let payload = Packet::deserialize(packet).unwrap().payload;
                        assert_eq!(payload.len(), payload_size);
                        packets_sent += 1;
                    }
                    OutputEvent::SfuUnpublishRequest(event) => {
                        assert_eq!(event.handle, pub_handle);
                        assert_eq!(packets_sent, packet_count);
                        break;
                    }
                }
            }
        };
        let publish_track = async {
            let track_options = DataTrackOptions::new(track_name.clone());
            let track = input.publish_track(track_options).await.unwrap();
            assert!(!track.info().uses_e2ee());
            assert_eq!(track.info().name(), track_name);
            assert_eq!(track.info().sid(), track_sid);

            for _ in 0..packet_count {
                track.try_push(vec![0xFA; payload_size].into()).unwrap();
                sleep(Duration::from_millis(10)).await;
            }
            // Only reference to track dropped here (unpublish)
        };
        timeout(Duration::from_secs(1), async { tokio::join!(publish_track, handle_events) })
            .await
            .unwrap();
    }
}
