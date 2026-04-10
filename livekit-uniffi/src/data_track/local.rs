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

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use inner::OutputEvent;
use livekit_datatrack::api::{DataTrack, DataTrackFrame, Local, PushFrameErrorReason};
use livekit_datatrack::backend::local::{
    publish_result_from_request_response, InputEvent, ManagerInput, SfuPublishResponse,
};
use livekit_datatrack::backend::EncryptionError;
use livekit_datatrack::{
    api::{DataTrackOptions, DataTrackSid, PublishError},
    backend::{local as inner, EncryptedPayload, InitializationVector},
};
use livekit_protocol as proto;
use prost::Message;
use std::sync::Arc;
use tokio_util::sync::{CancellationToken, DropGuard};

uniffi::custom_type!(DataTrackSid, String, {
    remote,
    lower: |s| String::from(s),
    try_lift: |s| DataTrackSid::try_from(s).map_err(|e| uniffi::deps::anyhow::anyhow!("{e}")),
});

#[uniffi::remote(Record)]
pub struct DataTrackOptions {
    pub name: String,
}

/// Information about a published data track.
#[derive(uniffi::Record)]
pub struct DataTrackInfo {
    pub sid: DataTrackSid,
    pub name: String,
    pub uses_e2ee: bool,
}

impl From<&livekit_datatrack::api::DataTrackInfo> for DataTrackInfo {
    fn from(info: &livekit_datatrack::api::DataTrackInfo) -> Self {
        Self { sid: info.sid(), name: info.name().to_string(), uses_e2ee: info.uses_e2ee() }
    }
}

#[uniffi::remote(Error)]
pub enum PushFrameErrorReason {
    TrackUnpublished,
    QueueFull,
}

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum PublishError {
    NotAllowed,
    DuplicateName,
    InvalidName,
    Timeout,
    LimitReached,
    Disconnected,
    Internal,
}

#[derive(uniffi::Object)]
pub struct LocalDataTrack(DataTrack<Local>);

#[uniffi::export]
impl LocalDataTrack {
    /// Whether or not the track is currently published.
    fn is_published(&self) -> bool {
        self.0.is_published()
    }

    /// Information about the data track.
    fn info(&self) -> DataTrackInfo {
        self.0.info().into()
    }

    /// Try pushing a frame to subscribers of the track.
    fn try_push(&self, frame: DataTrackFrame) -> Result<(), PushFrameErrorReason> {
        // `PushFrameError` returns ownership of the unpublished frame to the caller;
        // since this not applicable in an FFI context, just provide the reason.
        self.0.try_push(frame).map_err(|err| err.reason())
    }
}

/// System for managing data track publications.
#[derive(uniffi::Object)]
struct LocalDataTrackManager {
    input: inner::ManagerInput,
    _drop_guard: DropGuard,
}

#[uniffi::export]
impl LocalDataTrackManager {
    #[uniffi::constructor]
    pub fn new(
        delegate: Arc<dyn LocalDataTrackManagerDelegate>,
        e2ee_provider: Option<Arc<dyn LocalDataTrackEncryptionProvider>>,
    ) -> Arc<Self> {
        let token = CancellationToken::new();

        let manager_options = inner::ManagerOptions { encryption_provider: None };
        // TODO: encryption provider

        let (manager, input, output) = inner::Manager::new(manager_options);
        tokio::spawn(Self::shutdown_forward_task(input.clone(), token.clone()));
        tokio::spawn(Self::delegate_forward_task(output, delegate, token.clone()));
        tokio::spawn(manager.run());

        Self { input, _drop_guard: token.drop_guard() }.into()
    }
}

impl LocalDataTrackManager {
    async fn shutdown_forward_task(input: ManagerInput, token: CancellationToken) {
        // TODO: consider having manager work with cancellation token out-of-the-box.
        token.cancelled().await;
        _ = input.send(InputEvent::Shutdown);
    }

    async fn delegate_forward_task(
        output: impl Stream<Item = inner::OutputEvent>,
        delegate: Arc<dyn LocalDataTrackManagerDelegate>,
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

    fn forward_event(event: OutputEvent, delegate: &Arc<dyn LocalDataTrackManagerDelegate>) {
        match event {
            OutputEvent::PacketsAvailable(packets) => delegate.on_packets_available(packets),
            OutputEvent::SfuPublishRequest(req) => {
                let req = proto::signal_request::Message::PublishDataTrackRequest(req.into());
                Self::forward_signal_request(req, &delegate);
            }
            OutputEvent::SfuUnpublishRequest(req) => {
                let req = proto::signal_request::Message::UnpublishDataTrackRequest(req.into());
                Self::forward_signal_request(req, &delegate);
            }
        }
    }

    fn forward_signal_request(
        message: proto::signal_request::Message,
        delegate: &Arc<dyn LocalDataTrackManagerDelegate>,
    ) {
        let req = proto::SignalRequest { message: Some(message) }.encode_to_vec();
        delegate.on_signal_request(req);
    }
}

#[uniffi::export]
impl LocalDataTrackManager {
    /// Publishes a data track with given options.
    pub async fn publish_track(
        &self,
        options: DataTrackOptions,
    ) -> Result<LocalDataTrack, PublishError> {
        self.input.publish_track(options).await.map(|track| LocalDataTrack(track))
    }

    /// Get information about all currently published tracks.
    ///
    /// This does not include publications that are still pending.
    ///
    pub async fn query_tracks(&self) -> Vec<DataTrackInfo> {
        self.input.query_tracks().await.into_iter().map(|info| info.as_ref().into()).collect()
    }

    /// Republish all tracks.
    ///
    /// This must be invoked after a full reconnect in order for existing publications
    /// to be recognized by the SFU. Each republished track will be assigned a new SID.
    ///
    pub async fn republish_tracks(&self) {
        _ = self.input.send(inner::InputEvent::RepublishTracks);
    }

    /// Handles a serialized signal response from the SFU.
    ///
    /// This must be invoked for the following response types in order for the
    /// manager to function properly:
    ///
    /// - `RequestResponse`
    /// - `PublishDataTrackResponse`
    ///
    /// Invoking for other response types not listed above will log an error.
    ///
    pub fn handle_signal_response(&self, res: &[u8]) {
        // TODO: consider returning a result
        let Ok(Some(msg)) = proto::SignalResponse::decode(res).map(|res| res.message) else {
            log::error!("Failed to decode signal response");
            return;
        };

        use proto::signal_response::Message;
        let publish_res = match msg {
            Message::RequestResponse(msg) => {
                let Some(res) = publish_result_from_request_response(&msg) else {
                    // Not from data track publish request.
                    return;
                };
                res
            }
            Message::PublishDataTrackResponse(res) => {
                // TODO: handle error
                let res: SfuPublishResponse = res.try_into().unwrap();
                res
            }
            _ => {
                log::error!("Unsupported signal response type");
                return;
            }
        };

        let event: InputEvent = publish_res.into();
        _ = self.input.send(event);
    }
}

/// Delegate for receiving output events from [`LocalDataTrackManager`].
#[uniffi::export(with_foreign)]
pub trait LocalDataTrackManagerDelegate: Send + Sync {
    /// Encoded signal request to be forwarded to the SFU.
    fn on_signal_request(&self, request: Vec<u8>);

    /// Packets available to be sent over the data channel transport.
    fn on_packets_available(&self, packets: Vec<Bytes>);
}

uniffi::custom_type!(InitializationVector, Vec<u8>, {
    remote,
    lower: |iv| iv.to_vec(),
    try_lift: |v| v.try_into()
        .map_err(|_| uniffi::deps::anyhow::anyhow!("IV must be exactly 12 bytes"))
});

#[uniffi::remote(Record)]
pub struct EncryptedPayload {
    pub payload: Bytes,
    pub iv: InitializationVector,
    pub key_index: u8,
}

#[uniffi::remote(Error)]
#[uniffi(flat_error)]
pub enum EncryptionError {}

#[uniffi::export(with_foreign)]
pub trait LocalDataTrackEncryptionProvider: Send + Sync {
    /// Encrypts the given payload being sent by the local participant.
    fn encrypt(&self, payload: Bytes) -> Result<EncryptedPayload, EncryptionError>;
}
