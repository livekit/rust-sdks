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
    RemoteDataTrack, RemoteTrackInner,
};
use crate::{
    api::{DataTrackFrame, DataTrackInfo, DataTrackSid, DataTrackSubscribeError, InternalError},
    e2ee::DecryptionProvider,
    packet::{Handle, Packet},
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use std::{
    collections::{HashMap, HashSet},
    mem,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_stream::{wrappers::ReceiverStream, Stream};

/// Options for creating a [`Manager`].
#[derive(Debug)]
pub struct ManagerOptions {
    /// Provider to use for decrypting incoming frame payloads.
    ///
    /// If none, remote tracks using end-to-end encryption will not be available
    /// for subscription.
    ///
    pub decryption_provider: Option<Arc<dyn DecryptionProvider>>,
}

/// System for managing data track subscriptions.
pub struct Manager {
    decryption_provider: Option<Arc<dyn DecryptionProvider>>,
    event_in_tx: mpsc::Sender<InputEvent>,
    event_in_rx: mpsc::Receiver<InputEvent>,
    event_out_tx: mpsc::Sender<OutputEvent>,

    /// Mapping between track SID and descriptor.
    descriptors: HashMap<DataTrackSid, Descriptor>,

    /// Mapping between subscriber handle and track SID.
    ///
    /// This is an index that allows track descriptors to be looked up
    /// by subscriber handle in O(1) time—necessary for routing incoming packets.
    ///
    sub_handles: HashMap<Handle, DataTrackSid>,
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
    pub fn new(options: ManagerOptions) -> (Self, ManagerInput, ManagerOutput) {
        let (event_in_tx, event_in_rx) = mpsc::channel(Self::EVENT_BUFFER_COUNT);
        let (event_out_tx, event_out_rx) = mpsc::channel(Self::EVENT_BUFFER_COUNT);

        let event_in = ManagerInput::new(event_in_tx.clone());
        let manager = Manager {
            decryption_provider: options.decryption_provider,
            event_in_tx,
            event_in_rx,
            event_out_tx,
            descriptors: HashMap::default(),
            sub_handles: HashMap::default(),
        };

        let event_out = ManagerOutput(ReceiverStream::new(event_out_rx));
        (manager, event_in, event_out)
    }

    /// Run the manager task, consuming self.
    ///
    /// The manager will continue running until receiving [`InputEvent::Shutdown`].
    ///
    pub async fn run(mut self) {
        log::debug!("Task started");
        while let Some(event) = self.event_in_rx.recv().await {
            match event {
                InputEvent::SubscribeRequest(event) => self.on_subscribe_request(event).await,
                InputEvent::UnsubscribeRequest(event) => self.on_unsubscribe_request(event).await,
                InputEvent::SfuPublicationUpdates(event) => {
                    self.on_sfu_publication_updates(event).await
                }
                InputEvent::SfuSubscriberHandles(event) => self.on_sfu_subscriber_handles(event),
                InputEvent::PacketReceived(bytes) => self.on_packet_received(bytes),
                InputEvent::ResendSubscriptionUpdates => {
                    self.on_resend_subscription_updates().await
                }
                InputEvent::Shutdown => break,
            }
        }
        self.shutdown().await;
        log::debug!("Task ended");
    }

    async fn on_subscribe_request(&mut self, event: SubscribeRequest) {
        let Some(descriptor) = self.descriptors.get_mut(&event.sid) else {
            let error = DataTrackSubscribeError::Internal(
                anyhow!("Cannot subscribe to unknown track").into(),
            );
            _ = event.result_tx.send(Err(error));
            return;
        };
        match &mut descriptor.subscription {
            SubscriptionState::None => {
                let update_event = SfuUpdateSubscription { sid: event.sid, subscribe: true };
                _ = self.event_out_tx.send(update_event.into()).await;
                descriptor.subscription = SubscriptionState::Pending {
                    result_txs: vec![event.result_tx],
                    buffer_size: event.options.buffer_size,
                };
                // TODO: schedule timeout internally
            }
            SubscriptionState::Pending { result_txs, .. } => {
                result_txs.push(event.result_tx);
            }
            SubscriptionState::Active { frame_tx, .. } => {
                let frame_rx = frame_tx.subscribe();
                _ = event.result_tx.send(Ok(frame_rx))
            }
        }
    }

    async fn on_unsubscribe_request(&mut self, event: UnsubscribeRequest) {
        let Some(descriptor) = self.descriptors.get_mut(&event.sid) else {
            return;
        };

        let SubscriptionState::Active { sub_handle, .. } = descriptor.subscription else {
            log::warn!("Unexpected state");
            return;
        };
        descriptor.subscription = SubscriptionState::None;
        self.sub_handles.remove(&sub_handle);

        let event = SfuUpdateSubscription { sid: event.sid, subscribe: false };
        _ = self.event_out_tx.send(event.into()).await;
    }

    async fn on_sfu_publication_updates(&mut self, event: SfuPublicationUpdates) {
        if event.updates.is_empty() {
            return;
        }
        let mut participant_to_sids: HashMap<String, HashSet<DataTrackSid>> = HashMap::new();

        // Detect published tracks
        for (publisher_identity, tracks) in event.updates {
            let sids_in_update = participant_to_sids.entry(publisher_identity.clone()).or_default();
            for info in tracks {
                let sid = info.sid();
                sids_in_update.insert(sid.clone());
                if self.descriptors.contains_key(&sid) {
                    continue;
                }
                self.handle_track_published(publisher_identity.clone(), info).await;
            }
        }

        // Detect unpublished tracks (scoped per publisher in the update)
        for (publisher_identity, sids_in_update) in &participant_to_sids {
            let unpublished_sids: Vec<_> = self
                .descriptors
                .iter()
                .filter(|(_, desc)| desc.publisher_identity.as_ref() == publisher_identity)
                .filter(|(sid, _)| !sids_in_update.contains(*sid))
                .map(|(sid, _)| sid.clone())
                .collect();
            for sid in unpublished_sids {
                self.handle_track_unpublished(sid).await;
            }
        }
    }

    async fn handle_track_published(&mut self, publisher_identity: String, info: DataTrackInfo) {
        let sid = info.sid();
        if self.descriptors.contains_key(&sid) {
            log::error!("Existing descriptor for track {}", sid);
            return;
        }
        let info = Arc::new(info);
        let publisher_identity: Arc<str> = publisher_identity.into();

        let (published_tx, published_rx) = watch::channel(true);

        let descriptor = Descriptor {
            info: info.clone(),
            publisher_identity: publisher_identity.clone(),
            published_tx,
            subscription: SubscriptionState::None,
        };
        self.descriptors.insert(sid, descriptor);

        let inner = RemoteTrackInner {
            published_rx,
            event_in_tx: self.event_in_tx.downgrade(), // TODO: wrap
            publisher_identity,
        };
        let track = RemoteDataTrack::new(info, inner);
        _ = self.event_out_tx.send(TrackPublished { track }.into()).await;
    }

    async fn handle_track_unpublished(&mut self, sid: DataTrackSid) {
        let Some(descriptor) = self.descriptors.remove(&sid) else {
            log::error!("Unknown track {}", sid);
            return;
        };
        if let SubscriptionState::Active { sub_handle, .. } = descriptor.subscription {
            self.sub_handles.remove(&sub_handle);
        };
        _ = descriptor.published_tx.send(false);
        _ = self.event_out_tx.send(TrackUnpublished { sid }.into()).await;
    }

    fn on_sfu_subscriber_handles(&mut self, event: SfuSubscriberHandles) {
        for (handle, sid) in event.mapping {
            self.register_subscriber_handle(handle, sid);
        }
    }

    fn register_subscriber_handle(&mut self, assigned_handle: Handle, sid: DataTrackSid) {
        let Some(descriptor) = self.descriptors.get_mut(&sid) else {
            log::warn!("Unknown track: {}", sid);
            return;
        };
        let (result_txs, buffer_size) = match &mut descriptor.subscription {
            SubscriptionState::None => {
                // Handle assigned when there is no pending or active subscription is unexpected.
                log::warn!("No subscription for {}", sid);
                return;
            }
            SubscriptionState::Active { sub_handle, .. } => {
                // Update handle for an active subscription. This can occur following a full reconnect.
                self.sub_handles.remove(sub_handle);
                *sub_handle = assigned_handle;
                self.sub_handles.insert(assigned_handle, sid);
                return;
            }
            SubscriptionState::Pending { result_txs, buffer_size } => {
                // Handle assigned for pending subscription, transition to active.
                (mem::take(result_txs), *buffer_size)
            }
        };

        let (packet_tx, packet_rx) = mpsc::channel(Self::PACKET_BUFFER_COUNT);
        let (frame_tx, frame_rx) = broadcast::channel(buffer_size);

        let decryption_provider = if descriptor.info.uses_e2ee() {
            self.decryption_provider.as_ref().map(Arc::clone)
        } else {
            None
        };

        let pipeline_opts = PipelineOptions {
            info: descriptor.info.clone(),
            publisher_identity: descriptor.publisher_identity.clone(),
            decryption_provider,
        };
        let pipeline = Pipeline::new(pipeline_opts);

        let track_task = TrackTask {
            info: descriptor.info.clone(),
            pipeline,
            published_rx: descriptor.published_tx.subscribe(),
            packet_rx,
            frame_tx: frame_tx.clone(),
            event_in_tx: self.event_in_tx.clone(),
        };
        let task_handle = livekit_runtime::spawn(track_task.run());

        descriptor.subscription = SubscriptionState::Active {
            sub_handle: assigned_handle,
            packet_tx,
            frame_tx,
            task_handle,
        };
        self.sub_handles.insert(assigned_handle, sid);

        for result_tx in result_txs {
            _ = result_tx.send(Ok(frame_rx.resubscribe()));
        }
    }

    fn on_packet_received(&mut self, bytes: Bytes) {
        let packet = match Packet::deserialize(bytes) {
            Ok(packet) => packet,
            Err(err) => {
                log::error!("Failed to deserialize packet: {}", err);
                return;
            }
        };
        let Some(sid) = self.sub_handles.get(&packet.header.track_handle) else {
            log::warn!("Unknown subscriber handle {}", packet.header.track_handle);
            return;
        };
        let Some(descriptor) = self.descriptors.get(sid) else {
            log::warn!("Missing descriptor for track {}", sid);
            return;
        };
        let SubscriptionState::Active { packet_tx, .. } = &descriptor.subscription else {
            log::warn!("Received packet for track {} without subscription", sid);
            return;
        };
        _ = packet_tx
            .try_send(packet)
            .inspect_err(|err| log::debug!("Cannot send packet to track pipeline: {}", err));
    }

    async fn on_resend_subscription_updates(&self) {
        let update_events =
            self.descriptors.iter().filter_map(|(sid, descriptor)| match descriptor.subscription {
                SubscriptionState::None => None,
                SubscriptionState::Pending { .. } | SubscriptionState::Active { .. } => {
                    Some(SfuUpdateSubscription { sid: sid.clone(), subscribe: true })
                }
            });
        for event in update_events {
            _ = self.event_out_tx.send(event.into()).await;
        }
    }

    /// Performs cleanup before the task ends.
    async fn shutdown(self) {
        for (_, descriptor) in self.descriptors {
            _ = descriptor.published_tx.send(false);
            match descriptor.subscription {
                SubscriptionState::None => {}
                SubscriptionState::Pending { result_txs, .. } => {
                    for result_tx in result_txs {
                        _ = result_tx.send(Err(DataTrackSubscribeError::Disconnected));
                    }
                }
                SubscriptionState::Active { task_handle, .. } => task_handle.await,
            }
        }
    }

    /// Maximum number of incoming packets to buffer per track to be sent
    /// to the track's pipeline.
    const PACKET_BUFFER_COUNT: usize = 16;

    /// Maximum number of input and output events to buffer.
    const EVENT_BUFFER_COUNT: usize = 16;
}

/// Information and state for a remote data track.
#[derive(Debug)]
struct Descriptor {
    info: Arc<DataTrackInfo>,
    publisher_identity: Arc<str>,
    published_tx: watch::Sender<bool>,
    subscription: SubscriptionState,
}

#[derive(Debug)]
enum SubscriptionState {
    /// Track is not subscribed to.
    None,
    /// Track is being subscribed to, waiting for subscriber handle.
    Pending {
        /// All currently pending requests to subscribe to the track.
        result_txs: Vec<oneshot::Sender<SubscribeResult>>,
        /// Internal frame buffer size to use once active.
        buffer_size: usize,
    },
    /// Track has an active subscription.
    Active {
        sub_handle: Handle,
        packet_tx: mpsc::Sender<Packet>,
        frame_tx: broadcast::Sender<DataTrackFrame>,
        task_handle: livekit_runtime::JoinHandle<()>,
    },
}

/// Task for an individual data track with an active subscription.
struct TrackTask {
    info: Arc<DataTrackInfo>,
    pipeline: Pipeline,
    published_rx: watch::Receiver<bool>,
    packet_rx: mpsc::Receiver<Packet>,
    frame_tx: broadcast::Sender<DataTrackFrame>,
    event_in_tx: mpsc::Sender<InputEvent>,
}

impl TrackTask {
    async fn run(mut self) {
        log::debug!("Track task started: name={}", self.info.name);

        let mut is_published = *self.published_rx.borrow();
        while is_published {
            tokio::select! {
                biased;  // State updates take priority
                _ = self.published_rx.changed() => {
                    is_published = *self.published_rx.borrow();
                },
                _ = self.frame_tx.closed() => {
                    let event = UnsubscribeRequest { sid: self.info.sid() };
                    _ = self.event_in_tx.send(event.into()).await;
                    break;  // No more subscribers
                },
                Some(packet) = self.packet_rx.recv() => {
                    self.receive(packet);
                },
                else => break
            }
        }

        log::debug!("Track task ended: name={}", self.info.name);
    }

    fn receive(&mut self, packet: Packet) {
        let Some(frame) = self.pipeline.process_packet(packet) else { return };
        _ = self
            .frame_tx
            .send(frame)
            .inspect_err(|err| log::debug!("Cannot send frame to subscribers: {}", err));
    }
}

/// Channel for sending [`InputEvent`]s to [`Manager`].
#[derive(Debug, Clone)]
pub struct ManagerInput {
    event_in_tx: mpsc::Sender<InputEvent>,
    _drop_guard: Arc<DropGuard>,
}

/// Stream of [`OutputEvent`]s produced by [`Manager`].
#[derive(Debug)]
pub struct ManagerOutput(ReceiverStream<OutputEvent>);

impl Stream for ManagerOutput {
    type Item = OutputEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
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
        Ok(self.event_in_tx.try_send(event).context("Failed to send input event")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::DataTrackSubscribeOptions,
        e2ee::{DecryptionError, DecryptionProvider, EncryptedPayload},
        packet::{E2eeExt, Extensions, FrameMarker, Header, Timestamp},
        utils::testing::expect_event,
    };
    use fake::{Fake, Faker};
    use futures_util::{future::join, StreamExt};
    use std::{collections::HashMap, sync::RwLock, time::Duration};
    use test_case::test_case;
    use tokio::time;

    #[derive(Debug)]
    struct PrefixStrippingDecryptor;

    impl DecryptionProvider for PrefixStrippingDecryptor {
        fn decrypt(
            &self,
            payload: EncryptedPayload,
            _sender_identity: &str,
        ) -> Result<Bytes, DecryptionError> {
            Ok(payload.payload.slice(4..))
        }
    }

    #[tokio::test]
    async fn test_manager_task_shutdown() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, _) = Manager::new(options);

        let join_handle = livekit_runtime::spawn(manager.run());
        _ = input.send(InputEvent::Shutdown);

        time::timeout(Duration::from_secs(1), join_handle).await.unwrap();
    }

    #[test_case(true; "via_unpublish")]
    #[test_case(false; "via_unsubscribe")]
    #[tokio::test]
    async fn test_track_task_shutdown(via_unpublish: bool) {
        let mut info: DataTrackInfo = Faker.fake();
        info.uses_e2ee = false;

        let info = Arc::new(info);
        let sid = info.sid();
        let publisher_identity: Arc<str> = Faker.fake::<String>().into();

        let pipeline_opts =
            PipelineOptions { info: info.clone(), publisher_identity, decryption_provider: None };
        let pipeline = Pipeline::new(pipeline_opts);

        let (published_tx, published_rx) = watch::channel(true);
        let (_packet_tx, packet_rx) = mpsc::channel(4);
        let (frame_tx, frame_rx) = broadcast::channel(4);
        let (event_in_tx, mut event_in_rx) = mpsc::channel(4);

        let task =
            TrackTask { info: info, pipeline, published_rx, packet_rx, frame_tx, event_in_tx };
        let task_handle = livekit_runtime::spawn(task.run());

        let trigger_shutdown = async {
            if via_unpublish {
                // Simulates SFU publication update
                published_tx.send(false).unwrap();
                return;
            }
            // Simulates all subscribers dropped
            mem::drop(frame_rx);

            while let Some(event) = event_in_rx.recv().await {
                let InputEvent::UnsubscribeRequest(event) = event else {
                    panic!("Unexpected event type");
                };
                assert_eq!(event.sid, sid);
                return;
            }
            panic!("Did not receive unsubscribe");
        };
        time::timeout(Duration::from_secs(1), join(task_handle, trigger_shutdown)).await.unwrap();
    }

    #[tokio::test]
    async fn test_subscribe() {
        let publisher_identity: String = Faker.fake();
        let track_name: String = Faker.fake();
        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();

        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        // Simulate track published
        let event = SfuPublicationUpdates {
            updates: HashMap::from([(
                publisher_identity.clone(),
                vec![DataTrackInfo {
                    sid: RwLock::new(track_sid.clone()).into(),
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
                    OutputEvent::TrackPublished(track) => return track,
                    _ => continue,
                }
            }
            panic!("No track received");
        };

        let track = wait_for_track.await.track;
        assert!(track.is_published());
        assert_eq!(track.info().name, track_name);
        assert_eq!(track.info().sid(), track_sid);
        assert_eq!(track.publisher_identity(), publisher_identity);

        let simulate_subscriber_handles = async {
            while let Some(event) = output.next().await {
                match event {
                    OutputEvent::SfuUpdateSubscription(event) => {
                        assert!(event.subscribe);
                        assert_eq!(event.sid, track_sid);
                        time::sleep(Duration::from_millis(20)).await;

                        // Simulate SFU reply
                        let event = SfuSubscriberHandles {
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

    #[tokio::test]
    async fn test_track_publication_add_and_remove() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event =
            SfuPublicationUpdates { updates: HashMap::from([("identity1".into(), vec![info])]) };
        input.send(event.into()).unwrap();

        let track = expect_event!(output, OutputEvent::TrackPublished).track;
        assert_eq!(track.info().sid(), track_sid);
        assert_eq!(track.info().name, "test");
        assert!(track.is_published());

        // Simulate track unpublished
        let event =
            SfuPublicationUpdates { updates: HashMap::from([("identity1".into(), vec![])]) };
        input.send(event.into()).unwrap();

        time::timeout(Duration::from_secs(1), track.wait_for_unpublish()).await.unwrap();
        assert!(!track.is_published());

        let event = expect_event!(output, OutputEvent::TrackUnpublished);
        assert_eq!(event.sid, track_sid);
    }

    #[tokio::test]
    async fn test_sfu_publication_updates_idempotent() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate three identical publication updates
        for _ in 0..3 {
            let event = SfuPublicationUpdates {
                updates: HashMap::from([("identity1".into(), vec![info.clone()])]),
            };
            input.send(event.into()).unwrap();
        }

        expect_event!(output, OutputEvent::TrackPublished);

        // Drain remaining events; no second TrackAvailable should appear
        input.send(InputEvent::Shutdown).unwrap();
        while let Some(event) = output.next().await {
            assert!(!matches!(event, OutputEvent::TrackPublished(_)));
        }
    }

    #[tokio::test]
    async fn test_subscribe_receives_frame() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // Subscribe to the track
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);
        assert_eq!(event.sid, track_sid);

        // Simulate SFU assigning subscriber handle
        let event = SfuSubscriberHandles { mapping: HashMap::from([(sub_handle, track_sid)]) };
        input.send(event.into()).unwrap();

        let mut frame_rx =
            time::timeout(Duration::from_secs(1), result_rx).await.unwrap().unwrap().unwrap();

        // Simulate receiving a single-frame packet
        let packet = Packet {
            header: Header {
                marker: FrameMarker::Single,
                track_handle: sub_handle,
                sequence: 0,
                frame_number: 0,
                timestamp: Timestamp::from_ticks(0),
                extensions: Extensions::default(),
            },
            payload: Bytes::from_static(&[1, 2, 3, 4, 5]),
        };
        input.send(InputEvent::PacketReceived(packet.serialize())).unwrap();

        let frame = time::timeout(Duration::from_secs(1), frame_rx.recv()).await.unwrap().unwrap();
        assert_eq!(frame.payload.as_ref(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_subscribe_with_e2ee() {
        let options =
            ManagerOptions { decryption_provider: Some(Arc::new(PrefixStrippingDecryptor)) };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: true,
        };

        // Simulate track published (with e2ee)
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // Subscribe to the track
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);

        // Simulate SFU assigning subscriber handle
        let event = SfuSubscriberHandles { mapping: HashMap::from([(sub_handle, track_sid)]) };
        input.send(event.into()).unwrap();

        let mut frame_rx =
            time::timeout(Duration::from_secs(1), result_rx).await.unwrap().unwrap().unwrap();

        // Simulate receiving an encrypted single-frame packet
        let packet = Packet {
            header: Header {
                marker: FrameMarker::Single,
                track_handle: sub_handle,
                sequence: 0,
                frame_number: 0,
                timestamp: Timestamp::from_ticks(0),
                extensions: Extensions {
                    e2ee: Some(E2eeExt { key_index: 0, iv: [0; 12] }),
                    ..Default::default()
                },
            },
            payload: Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4, 5]),
        };
        input.send(InputEvent::PacketReceived(packet.serialize())).unwrap();

        // Payload should have fake encryption prefix stripped by decryptor
        let frame = time::timeout(Duration::from_secs(1), frame_rx.recv()).await.unwrap().unwrap();
        assert_eq!(frame.payload.as_ref(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_subscribe_fan_out_to_multiple_subscribers() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // First subscriber triggers SFU interaction
        let (result_tx1, result_rx1) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx: result_tx1,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);

        // Simulate SFU assigning subscriber handle
        let event =
            SfuSubscriberHandles { mapping: HashMap::from([(sub_handle, track_sid.clone())]) };
        input.send(event.into()).unwrap();

        let mut rx1 =
            time::timeout(Duration::from_secs(1), result_rx1).await.unwrap().unwrap().unwrap();

        // Additional subscribers attach directly (no further SFU interaction)
        let (result_tx2, result_rx2) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx: result_tx2,
        };
        input.send(event.into()).unwrap();
        let mut rx2 = result_rx2.await.unwrap().unwrap();

        let (result_tx3, result_rx3) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx: result_tx3,
        };
        input.send(event.into()).unwrap();
        let mut rx3 = result_rx3.await.unwrap().unwrap();

        // Simulate receiving a single-frame packet
        let packet = Packet {
            header: Header {
                marker: FrameMarker::Single,
                track_handle: sub_handle,
                sequence: 0,
                frame_number: 0,
                timestamp: Timestamp::from_ticks(0),
                extensions: Extensions::default(),
            },
            payload: Bytes::from_static(&[1, 2, 3, 4, 5]),
        };
        input.send(InputEvent::PacketReceived(packet.serialize())).unwrap();

        // All subscribers should receive the same frame
        for rx in [&mut rx1, &mut rx2, &mut rx3] {
            let frame = time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
            assert_eq!(frame.payload.as_ref(), &[1, 2, 3, 4, 5]);
        }
    }

    #[tokio::test]
    async fn test_subscribe_unknown_track_fails() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, _) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        // Subscribe to a track that was never published
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: Faker.fake(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let result = result_rx.await.unwrap();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unpublish_terminates_pending_subscription() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // Subscribe (enters Pending state)
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);

        // Simulate track unpublished before SFU assigns a handle
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![])]) };
        input.send(event.into()).unwrap();

        let result = time::timeout(Duration::from_secs(1), result_rx).await.unwrap();
        assert!(result.is_err());

        let event = expect_event!(output, OutputEvent::TrackUnpublished);
        assert_eq!(event.sid, track_sid);
    }

    #[tokio::test]
    async fn test_unpublish_terminates_active_subscription() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // Subscribe to the track
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);

        // Simulate SFU assigning subscriber handle
        let event =
            SfuSubscriberHandles { mapping: HashMap::from([(sub_handle, track_sid.clone())]) };
        input.send(event.into()).unwrap();

        let mut frame_rx =
            time::timeout(Duration::from_secs(1), result_rx).await.unwrap().unwrap().unwrap();

        // Simulate track unpublished while subscription is active
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![])]) };
        input.send(event.into()).unwrap();

        let result = time::timeout(Duration::from_secs(1), frame_rx.recv()).await.unwrap();
        assert!(result.is_err());

        let event = expect_event!(output, OutputEvent::TrackUnpublished);
        assert_eq!(event.sid, track_sid);
    }

    #[tokio::test]
    async fn test_all_subscribers_dropped_terminates_sfu_subscription() {
        let options = ManagerOptions { decryption_provider: None };
        let (manager, input, mut output) = Manager::new(options);
        livekit_runtime::spawn(manager.run());

        let track_sid: DataTrackSid = Faker.fake();
        let sub_handle: Handle = Faker.fake();
        let info = DataTrackInfo {
            sid: RwLock::new(track_sid.clone()).into(),
            pub_handle: Faker.fake(),
            name: "test".into(),
            uses_e2ee: false,
        };

        // Simulate track published
        let event = SfuPublicationUpdates { updates: HashMap::from([("id".into(), vec![info])]) };
        input.send(event.into()).unwrap();
        expect_event!(output, OutputEvent::TrackPublished);

        // Subscribe to the track
        let (result_tx, result_rx) = oneshot::channel();
        let event = SubscribeRequest {
            sid: track_sid.clone(),
            options: DataTrackSubscribeOptions::default(),
            result_tx,
        };
        input.send(event.into()).unwrap();

        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(event.subscribe);

        // Simulate SFU assigning subscriber handle
        let event =
            SfuSubscriberHandles { mapping: HashMap::from([(sub_handle, track_sid.clone())]) };
        input.send(event.into()).unwrap();

        let frame_rx =
            time::timeout(Duration::from_secs(1), result_rx).await.unwrap().unwrap().unwrap();

        // Drop the only subscriber
        drop(frame_rx);

        // Manager should request SFU to unsubscribe
        let event = expect_event!(output, OutputEvent::SfuUpdateSubscription);
        assert!(!event.subscribe);
        assert_eq!(event.sid, track_sid);
    }
}
