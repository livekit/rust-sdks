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
    e2ee::{FfiDecryptionProvider, DataTrackDecryptionProvider},
    DataTrackInfo, DataTrackSignalResponseError,
};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use livekit_datatrack::{
    api::{DataTrack, DataTrackFrame, DataTrackSid, DataTrackSubscribeError, Remote},
    backend::{
        remote::{self as inner, event_from_participant_update},
        DecryptionProvider,
    },
};
use livekit_protocol as proto;
use prost::Message;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::{CancellationToken, DropGuard};

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum DataTrackSubscribeError {
    Unpublished,
    Timeout,
    Disconnected,
    Internal,
}

/// Data track published by the local participant.
#[derive(uniffi::Object)]
pub struct RemoteDataTrack(DataTrack<Remote>);

#[uniffi::export]
impl RemoteDataTrack {
    /// Whether or not the track is currently published.
    fn is_published(&self) -> bool {
        self.0.is_published()
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
    input: inner::ManagerInput,
    _drop_guard: DropGuard,
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
        let manager_options = inner::ManagerOptions { decryption_provider };

        let (manager, input, output) = inner::Manager::new(manager_options);
        tokio::spawn(Self::shutdown_forward_task(input.clone(), token.clone()));
        tokio::spawn(Self::delegate_forward_task(output, delegate, token.clone()));
        tokio::spawn(manager.run());

        Self { input, _drop_guard: token.drop_guard() }.into()
    }
}

impl RemoteDataTrackManager {
    async fn shutdown_forward_task(input: inner::ManagerInput, token: CancellationToken) {
        // TODO: consider having manager work with cancellation token out-of-the-box.
        token.cancelled().await;
        _ = input.send(inner::InputEvent::Shutdown);
    }

    async fn delegate_forward_task(
        output: impl Stream<Item = inner::OutputEvent>,
        delegate: Arc<dyn RemoteDataTrackManagerDelegate>,
        token: CancellationToken,
    ) {
        tokio::pin!(output);
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                Some(event) = output.next() => Self::forward_event(event, &delegate)
            }
        }
    }

    fn forward_event(
        event: inner::OutputEvent,
        delegate: &Arc<dyn RemoteDataTrackManagerDelegate>,
    ) {
        match event {
            inner::OutputEvent::SfuUpdateSubscription(req) => {
                let req = proto::signal_request::Message::UpdateDataSubscription(req.into());
                Self::forward_signal_request(req, delegate);
            }
            inner::OutputEvent::TrackPublished(event) => {
                let track = Arc::new(RemoteDataTrack(event.track));
                delegate.on_track_published(track);
            }
            inner::OutputEvent::TrackUnpublished(event) => delegate.on_track_unpublished(event.sid),
        }
    }

    fn forward_signal_request(
        message: proto::signal_request::Message,
        delegate: &Arc<dyn RemoteDataTrackManagerDelegate>,
    ) {
        let req = proto::SignalRequest { message: Some(message) }.encode_to_vec();
        delegate.on_signal_request(req);
    }
}

#[uniffi::export]
impl RemoteDataTrackManager {
    /// Resend all subscription updates.
    ///
    /// This must be sent after a full reconnect to ensure the SFU knows which tracks
    /// are subscribed to locally.
    ///
    pub fn resend_subscription_updates(&self) {
        _ = self.input.send(inner::InputEvent::ResendSubscriptionUpdates);
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
    ) -> Result<(), DataTrackSignalResponseError> {
        let res = proto::SignalResponse::decode(res)
            .map_err(|err| DataTrackSignalResponseError::Decode(err))?;

        let msg = res.message.ok_or(DataTrackSignalResponseError::EmptyMessage)?;

        use proto::signal_response::Message;
        match msg {
            Message::Update(mut msg) => {
                let event = event_from_participant_update(&mut msg, &local_participant_identity)
                    .map_err(|err| DataTrackSignalResponseError::Internal(err))?;
                _ = self.input.send(event.into());
            }
            Message::DataTrackSubscriberHandles(msg) => {
                let event: inner::SfuSubscriberHandles =
                    msg.try_into().map_err(|err| DataTrackSignalResponseError::Internal(err))?;
                _ = self.input.send(event.into())
            }
            _ => {
                return Err(DataTrackSignalResponseError::UnsupportedType);
            }
        };
        Ok(())
    }

    /// Handles a encoded packet received over the data channel.
    pub fn handle_packet_received(&self, packet: Bytes) {
        _ = self.input.send(inner::InputEvent::PacketReceived(packet))
    }
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
