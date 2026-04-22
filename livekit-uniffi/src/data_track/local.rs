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
    e2ee::{DataTrackEncryptionProvider, FfiEncryptionProvider},
};
use bytes::Bytes;
use futures_util::StreamExt;
use livekit_datatrack::{
    api::{DataTrack, DataTrackFrame, DataTrackOptions, Local, PublishError, PushFrameErrorReason},
    backend::{local, EncryptionProvider},
};
use livekit_protocol as proto;
use prost::Message;
use std::sync::Arc;
use tokio_util::sync::{CancellationToken, DropGuard};

/// Data track published by the local participant.
#[derive(uniffi::Object)]
pub struct LocalDataTrack(DataTrack<Local>);

#[uniffi::export]
impl LocalDataTrack {
    /// Whether or not the track is currently published.
    pub fn is_published(&self) -> bool {
        self.0.is_published()
    }

    /// Waits asynchronously until the track is unpublished.
    ///
    /// Use this to trigger follow-up work once the track is no longer published.
    /// If the track is already unpublished, this method returns immediately.
    ///
    pub async fn wait_for_unpublish(&self) {
        self.0.wait_for_unpublish().await
    }

    /// Information about the data track.
    pub fn info(&self) -> DataTrackInfo {
        self.0.info().into()
    }

    /// Try pushing a frame to subscribers of the track.
    pub fn try_push(&self, frame: DataTrackFrame) -> Result<(), PushFrameErrorReason> {
        // `PushFrameError` returns ownership of the unpublished frame to the caller;
        // since this isn't applicable in an FFI context, just provide the reason.
        self.0.try_push(frame).map_err(|err| err.reason())
    }

    /// Unpublishes the track.
    pub fn unpublish(&self) {
        self.0.unpublish();
    }
}

#[uniffi::remote(Error)]
pub enum PushFrameErrorReason {
    TrackUnpublished,
    QueueFull,
}

#[uniffi::remote(Record)]
pub struct DataTrackOptions {
    pub name: String,
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

/// System for managing data track publications.
#[derive(uniffi::Object)]
struct LocalDataTrackManager {
    input: local::ManagerInput,
    _guard: DropGuard,
}

/// Delegate for receiving output events from [`LocalDataTrackManager`].
#[uniffi::export(with_foreign)]
pub trait LocalDataTrackManagerDelegate: Send + Sync {
    /// Encoded signal request to be forwarded to the SFU.
    fn on_signal_request(&self, request: Vec<u8>);

    /// Packets available to be sent over the data channel transport.
    fn on_packets_available(&self, packets: Vec<Bytes>);
}

#[uniffi::export]
impl LocalDataTrackManager {
    #[uniffi::constructor]
    pub fn new(
        delegate: Arc<dyn LocalDataTrackManagerDelegate>,
        encryption_provider: Option<Arc<dyn DataTrackEncryptionProvider>>,
    ) -> Arc<Self> {
        let token = CancellationToken::new();

        let encryption_provider = encryption_provider
            .map(|p| Arc::new(FfiEncryptionProvider(p)) as Arc<dyn EncryptionProvider>);
        let manager_options = local::ManagerOptions { encryption_provider };

        let (manager, input, output) = local::Manager::new(manager_options);

        let rt = crate::runtime::runtime();

        // TODO: in a follow-up PR, refactor manager to work with cancellation tokens directly, eliminating the
        // need for this additional task.
        rt.spawn(shutdown_forward_task(input.clone(), token.clone()));

        let delegate_forward = DelegateForwardTask { output, delegate, token: token.clone() };
        rt.spawn(delegate_forward.run());

        rt.spawn(manager.run());

        Self { input, _guard: token.drop_guard() }.into()
    }

    /// Publishes a data track with given options.
    pub async fn publish_track(
        &self,
        options: DataTrackOptions,
    ) -> Result<LocalDataTrack, PublishError> {
        self.input.publish_track(options).await.map(LocalDataTrack)
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
    pub fn republish_tracks(&self) {
        _ = self.input.send(local::InputEvent::RepublishTracks);
    }

    /// Handles a serialized signal response from the SFU.
    ///
    /// This must be invoked for the following response types in order for the
    /// manager to function properly:
    ///
    /// - `RequestResponse`
    /// - `PublishDataTrackResponse`
    ///
    /// If a signal response type not listed above is provided, the result is an error.
    ///
    pub fn handle_signal_response(&self, res: &[u8]) -> Result<(), HandleSignalResponseError> {
        let res = proto::SignalResponse::decode(res)
            .map_err(|err| HandleSignalResponseError::Decode(err))?;

        let msg = res.message.ok_or(HandleSignalResponseError::EmptyMessage)?;

        use proto::signal_response::Message;
        let publish_res = match msg {
            Message::RequestResponse(msg) => {
                let Some(res) = local::publish_result_from_request_response(&msg) else {
                    // Not from data track publish request.
                    return Ok(());
                };
                res
            }
            Message::PublishDataTrackResponse(res) => {
                let res: local::SfuPublishResponse =
                    res.try_into().map_err(|err| HandleSignalResponseError::Internal(err))?;
                res
            }
            _ => return Err(HandleSignalResponseError::UnsupportedType),
        };

        let event: local::InputEvent = publish_res.into();
        _ = self.input.send(event);

        Ok(())
    }
}

/// Task for forwarding manager output events to the foreign [`LocalDataTrackManagerDelegate`].
struct DelegateForwardTask {
    output: local::ManagerOutput,
    delegate: Arc<dyn LocalDataTrackManagerDelegate>,
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

    fn forward_event(&self, event: local::OutputEvent) {
        match event {
            local::OutputEvent::PacketsAvailable(packets) => {
                self.delegate.on_packets_available(packets)
            }
            local::OutputEvent::SfuPublishRequest(req) => {
                let req = proto::signal_request::Message::PublishDataTrackRequest(req.into());
                self.forward_signal_request(req);
            }
            local::OutputEvent::SfuUnpublishRequest(req) => {
                let req = proto::signal_request::Message::UnpublishDataTrackRequest(req.into());
                self.forward_signal_request(req);
            }
        }
    }

    fn forward_signal_request(&self, message: proto::signal_request::Message) {
        let req = proto::SignalRequest { message: Some(message) }.encode_to_vec();
        self.delegate.on_signal_request(req);
    }
}

async fn shutdown_forward_task(input: local::ManagerInput, token: CancellationToken) {
    token.cancelled().await;
    _ = input.send(local::InputEvent::Shutdown);
}
