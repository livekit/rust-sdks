// Copyright 2026 LiveKit, Inc.
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
    common::{DataTrackInfo, HandleSignalResponseError},
    e2ee::{DataTrackDecryptionProvider, FfiDecryptionProvider},
};
use bytes::Bytes;
use futures_util::StreamExt;
use livekit_datatrack::{
    api::{DataTrack, DataTrackFrame, DataTrackSid, DataTrackSubscribeError, Remote},
    backend::{remote, DecryptionProvider},
};
use livekit_protocol as proto;
use prost::Message;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::{CancellationToken, DropGuard};

/// Data track published by the local participant.
#[derive(uniffi::Object)]
pub struct RemoteDataTrack(DataTrack<Remote>);

#[uniffi::export]
impl RemoteDataTrack {
    /// Whether or not the track is currently published.
    fn is_published(&self) -> bool {
        self.0.is_published()
    }

    /// Waits asynchronously until the track is unpublished.
    ///
    /// Use this to trigger follow-up work once the track is no longer published.
    /// If the track is already unpublished, this method returns immediately.
    ///
    async fn wait_for_unpublish(&self) {
        self.0.wait_for_unpublish().await
    }

    /// Information about the data track.
    fn info(&self) -> DataTrackInfo {
        self.0.info().into()
    }

    /// Identity of the participant who published the track.
    fn publisher_identity(&self) -> String {
        self.0.publisher_identity().to_string()
    }

    /// Subscribes to the data track.
    async fn subscribe(&self) -> Result<DataTrackStream, DataTrackSubscribeError> {
        self.0.subscribe().await.map(|stream| DataTrackStream(Mutex::new(stream)))
    }
}

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum DataTrackSubscribeError {
    Unpublished,
    Timeout,
    Disconnected,
    Internal,
}

/// A stream of [`DataTrackFrame`]s received from a [`RemoteDataTrack`].
#[derive(uniffi::Object)]
struct DataTrackStream(Mutex<livekit_datatrack::api::DataTrackStream>);

#[uniffi::export]
impl DataTrackStream {
    async fn next(&self) -> Option<DataTrackFrame> {
        // TODO: avoid mutex?
        self.0.try_lock().unwrap().next().await
    }
}

/// System for managing data track subscriptions.
#[derive(uniffi::Object)]
struct RemoteDataTrackManager {
    input: remote::ManagerInput,
    _guard: DropGuard,
}

/// Delegate for receiving output events from [`RemoteDataTrackManager`].
#[uniffi::export(with_foreign)]
pub trait RemoteDataTrackManagerDelegate: Send + Sync {
    /// Encoded signal request to be forwarded to the SFU.
    fn on_signal_request(&self, request: Vec<u8>);

    /// A track has been published by a remote participant and is available to be
    /// subscribed to.
    ///
    /// Emit a public event to deliver the track to the user, allowing them to subscribe
    /// with [`RemoteDataTrack::subscribe`] if desired.
    ///
    fn on_track_published(&self, track: Arc<RemoteDataTrack>);

    /// A track with the given SID has been unpublished by a remote participant.
    fn on_track_unpublished(&self, sid: DataTrackSid);
}

#[uniffi::export]
impl RemoteDataTrackManager {
    #[uniffi::constructor]
    pub fn new(
        delegate: Arc<dyn RemoteDataTrackManagerDelegate>,
        decryption_provider: Option<Arc<dyn DataTrackDecryptionProvider>>,
    ) -> Arc<Self> {
        let token = CancellationToken::new();

        let decryption_provider = decryption_provider
            .map(|p| Arc::new(FfiDecryptionProvider(p)) as Arc<dyn DecryptionProvider>);
        let manager_options = remote::ManagerOptions { decryption_provider };

        let (manager, input, output) = remote::Manager::new(manager_options);

        // TODO: in a follow-up PR, refactor manager to work with cancellation tokens directly, eliminating the
        // need for this additional task.
        tokio::spawn(shutdown_forward_task(input.clone(), token.clone()));

        let delegate_forward = DelegateForwardTask { output, delegate, token: token.clone() };
        tokio::spawn(delegate_forward.run());

        tokio::spawn(manager.run());

        Self { input, _guard: token.drop_guard() }.into()
    }

    /// Resend all subscription updates.
    ///
    /// This must be sent after a full reconnect to ensure the SFU knows which tracks
    /// are subscribed to locally.
    ///
    pub fn resend_subscription_updates(&self) {
        _ = self.input.send(remote::InputEvent::ResendSubscriptionUpdates);
    }

    /// Handles a serialized signal response from the SFU.
    ///
    /// This must be invoked for the following response types in order for the
    /// manager to function properly:
    ///
    /// - `ParticipantUpdate`
    /// - `DataTrackSubscriberHandles`
    ///
    /// Note: the local participant identity is required to exclude data tracks published by the
    /// local participant from being treated as remote tracks.
    ///
    pub fn handle_signal_response(
        &self,
        res: &[u8],
        local_participant_identity: String,
    ) -> Result<(), HandleSignalResponseError> {
        let res = proto::SignalResponse::decode(res)
            .map_err(|err| HandleSignalResponseError::Decode(err))?;

        let msg = res.message.ok_or(HandleSignalResponseError::EmptyMessage)?;

        use proto::signal_response::Message;
        match msg {
            Message::Update(mut msg) => {
                let event =
                    remote::event_from_participant_update(&mut msg, &local_participant_identity)
                        .map_err(|err| HandleSignalResponseError::Internal(err))?;
                _ = self.input.send(event.into());
            }
            Message::DataTrackSubscriberHandles(msg) => {
                let event: remote::SfuSubscriberHandles =
                    msg.try_into().map_err(|err| HandleSignalResponseError::Internal(err))?;
                _ = self.input.send(event.into())
            }
            _ => {
                return Err(HandleSignalResponseError::UnsupportedType);
            }
        };
        Ok(())
    }

    /// Handles a encoded packet received over the data channel.
    pub fn handle_packet_received(&self, packet: Bytes) {
        _ = self.input.send(remote::InputEvent::PacketReceived(packet))
    }
}

/// Task for forwarding manger output events to the foreign [`RemoteDataTrackManagerDelegate`].
struct DelegateForwardTask {
    output: remote::ManagerOutput,
    delegate: Arc<dyn RemoteDataTrackManagerDelegate>,
    token: CancellationToken,
}

impl DelegateForwardTask {
    async fn run(mut self) {
        loop {
            tokio::select! {
                _ = self.token.cancelled() => break,
                Some(event) = self.output.next() => self.forward_event(event)
            }
        }
    }

    fn forward_event(&self, event: remote::OutputEvent) {
        match event {
            remote::OutputEvent::TrackPublished(event) => {
                let track = Arc::new(RemoteDataTrack(event.track));
                self.delegate.on_track_published(track);
            }
            remote::OutputEvent::TrackUnpublished(event) => {
                self.delegate.on_track_unpublished(event.sid)
            }
            remote::OutputEvent::SfuUpdateSubscription(req) => {
                let req = proto::signal_request::Message::UpdateDataSubscription(req.into());
                self.forward_signal_request(req);
            }
        }
    }

    fn forward_signal_request(&self, message: proto::signal_request::Message) {
        let req = proto::SignalRequest { message: Some(message) }.encode_to_vec();
        self.delegate.on_signal_request(req);
    }
}

async fn shutdown_forward_task(input: remote::ManagerInput, token: CancellationToken) {
    token.cancelled().await;
    _ = input.send(remote::InputEvent::Shutdown);
}
