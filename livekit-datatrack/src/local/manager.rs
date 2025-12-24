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
    track::{LocalTrackInner, LocalTrackTask},
    Local,
};
use crate::dtp::TrackHandle;
use crate::{
    dtp, DataTrack, DataTrackFrame, DataTrackInfo, DataTrackOptions, DataTrackState,
    EncryptionProvider, InternalError, PublishError, PublishFrameError, PublishFrameErrorReason,
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use from_variants::FromVariants;
use futures_util::Stream;
use livekit_protocol as proto;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug)]
pub struct PubManagerOptions {
    pub encryption: Option<Arc<dyn EncryptionProvider>>,
}

/// Manager for local data tracks.
#[derive(Debug, Clone)]
pub struct Manager {
    signal_in_tx: mpsc::Sender<PubSignalInput>,
    pub_req_tx: mpsc::Sender<PubRequest>,
}

impl Manager {
    const CH_BUFFER_SIZE: usize = 4;
    const PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new(
        options: PubManagerOptions,
    ) -> (Self, ManagerTask, impl Stream<Item = PubSignalOutput>, impl Stream<Item = Bytes>) {
        let (pub_req_tx, pub_req_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (signal_in_tx, signal_in_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (signal_out_tx, signal_out_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);
        let (packet_out_tx, packet_out_rx) = mpsc::channel(Self::CH_BUFFER_SIZE);

        let manager = Self { signal_in_tx, pub_req_tx };
        let task = ManagerTask {
            encryption: options.encryption,
            pub_req_rx,
            signal_in_rx,
            signal_out_tx,
            packet_out_tx,
            handle_allocator: dtp::TrackHandleAllocator::default(),
            pending_publications: HashMap::new(),
            active_publications: HashMap::new(),
        };

        let signal_out_stream = ReceiverStream::new(signal_out_rx);
        let packet_out_stream = ReceiverStream::new(packet_out_rx);

        (manager, task, signal_out_stream, packet_out_stream)
    }

    /// Handles a signal message from the SFU.
    ///
    /// In order to function correctly, all message types enumerated in [`PubSignalInput`]
    /// must be forwarded here.
    ///
    pub fn handle_signal(&self, message: PubSignalInput) -> Result<(), InternalError> {
        Ok(self.signal_in_tx.try_send(message).context("Failed to handle signal input")?)
    }

    /// Publishes a data track with the given options.
    pub async fn publish_track(
        &self,
        options: DataTrackOptions,
    ) -> Result<DataTrack<Local>, PublishError> {
        let (result_tx, result_rx) = oneshot::channel();
        let request = PubRequest { options, result_tx };
        self.pub_req_tx.try_send(request).map_err(|_| PublishError::Disconnected)?;

        // TODO: move timeout inside pub manager
        let track = timeout(Self::PUBLISH_TIMEOUT, result_rx)
            .await
            .map_err(|_| PublishError::Timeout)?
            .map_err(|_| PublishError::Disconnected)??;
        Ok(track)
    }
}

struct PubRequest {
    options: DataTrackOptions,
    result_tx: oneshot::Sender<Result<DataTrack<Local>, PublishError>>,
}

pub struct ManagerTask {
    encryption: Option<Arc<dyn EncryptionProvider>>,
    pub_req_rx: mpsc::Receiver<PubRequest>,
    signal_in_rx: mpsc::Receiver<PubSignalInput>,
    signal_out_tx: mpsc::Sender<PubSignalOutput>,
    packet_out_tx: mpsc::Sender<Bytes>,
    handle_allocator: dtp::TrackHandleAllocator,
    pending_publications:
        HashMap<TrackHandle, oneshot::Sender<Result<DataTrack<Local>, PublishError>>>,
    active_publications: HashMap<TrackHandle, watch::Sender<DataTrackState>>,
}

impl ManagerTask {
    pub async fn run(mut self) -> Result<(), InternalError> {
        loop {
            tokio::select! {
                biased; // Handle signal input before publish requests.
                // TODO: check cancellation
                Some(signal) = self.signal_in_rx.recv() => self.handle_signal(signal),
                //Some(unpublish_req) = self.unpub_req_rx.recv() => self.handle_unpublish_req(unpublish_req),
                Some(publish_req) = self.pub_req_rx.recv() => self.handle_publish_req(publish_req),
                else => Ok(())
            }
            .inspect_err(|err| log::error!("{}", err))
            .ok();
        }
    }

    fn handle_publish_req(&mut self, req: PubRequest) -> Result<(), InternalError> {
        let Some(handle) = self.handle_allocator.get() else {
            _ = req.result_tx.send(Err(PublishError::LimitReached));
            return Ok(());
        };

        if self.pending_publications.insert(handle, req.result_tx).is_some() {
            Err(anyhow!("Publication already pending for handle"))?
        }

        let use_e2ee = self.encryption.is_some() && !req.options.disable_e2ee;
        let request = req.options.into_add_track_request(use_e2ee, handle);
        self.signal_out_tx.try_send(request.into()).context("Failed to send add track")?;
        Ok(())
    }

    fn handle_signal(&mut self, message: PubSignalInput) -> Result<(), InternalError> {
        match message {
            PubSignalInput::PublishResponse(res) => self.handle_publish_response(res),
            PubSignalInput::UnpublishResponse(res) => self.handle_unpublish_response(res),
            PubSignalInput::RequestResponse(res) => self.handle_request_response(res),
            PubSignalInput::SyncState(res) => self.handle_sync_state(res),
        }
    }

    fn handle_publish_response(
        &mut self,
        res: proto::PublishDataTrackResponse,
    ) -> Result<(), InternalError> {
        let info: DataTrackInfo = res.try_into()?;
        let Some(res_tx) = self.pending_publications.remove(&info.handle) else {
            Err(anyhow!("No pending track publication for {}", info.handle))?
        };
        let track = self.create_local_track(info);
        let _ = res_tx.send(Ok(track));
        Ok(())
    }

    fn create_local_track(&mut self, info: DataTrackInfo) -> DataTrack<Local> {
        let (frame_tx, frame_rx) = mpsc::channel(4); // TODO: tune
        let (state_tx, state_rx) = watch::channel(DataTrackState::Published);
        let info = Arc::new(info);

        let task = LocalTrackTask {
            // TODO: handle cancellation
            packetizer: dtp::Packetizer::new(info.handle, 16_000),
            encryption: self.encryption.clone(),
            info: info.clone(),
            frame_rx,
            state_rx,
            packet_out_tx: self.packet_out_tx.clone(),
            signal_out_tx: self.signal_out_tx.clone(),
        };
        livekit_runtime::spawn(task.run());
        self.active_publications.insert(info.handle, state_tx.clone());

        let handle = LocalTrackInner { frame_tx, state_tx };
        DataTrack::<Local>::new(info, handle)
    }

    fn handle_request_response(
        &mut self,
        res: proto::RequestResponse,
    ) -> Result<(), InternalError> {
        let reason = res.reason();
        let req = res.request.context("Missing request")?;
        use proto::request_response::Request;
        match req {
            Request::PublishDataTrack(req) => {
                let handle: TrackHandle =
                    req.pub_handle.try_into().context("Invalid track handle")?;
                let Some(res_tx) = self.pending_publications.remove(&handle) else {
                    Err(anyhow!("No pending publication for {}", req.pub_handle))?
                };
                let error: PublishError = reason.into();
                _ = res_tx.send(Err(error));
            }
            Request::UnpublishDataTrack(req) => {
                log::warn!("Unpublish failed for {}", req.pub_handle)
            }
            _ => {} // Not handled by this module
        }
        Ok(())
    }

    fn handle_unpublish_response(
        &mut self,
        res: proto::UnpublishDataTrackResponse,
    ) -> Result<(), InternalError> {
        let handle = {
            let info: DataTrackInfo =
                res.info.context("Missing info")?.try_into().context("Invalid info")?;
            info.handle
        };
        let Some(state_tx) = self.active_publications.remove(&handle) else {
            Err(anyhow!("Cannot handle unpublish for unknown track {}", handle))?
        };
        let state = *state_tx.borrow();
        match state {
            DataTrackState::Published => {
                state_tx
                    .send(DataTrackState::Unpublished { sfu_initiated: true })
                    .context("Failed to set state")?;
            }
            DataTrackState::Unpublished { sfu_initiated } => {
                if sfu_initiated {
                    Err(anyhow!("Received unpublish response for same track more than once"))?
                }
            }
        }
        Ok(())
    }

    fn handle_sync_state(&mut self, res: proto::SyncState) -> Result<(), InternalError> {
        for res in res.publish_data_tracks {
            // Forward to standard response handler
            self.handle_publish_response(res)?
        }
        Ok(())
    }
}

#[derive(Debug, FromVariants)]
pub enum PubManagerInput {
    Signal(PubSignalInput),
    Transport(Bytes),
}

/// Signal message produced by [`PubManager`] to be forwarded to the SFU.
#[derive(Debug, FromVariants)]
pub enum PubSignalOutput {
    PublishRequest(proto::PublishDataTrackRequest),
    UnpublishRequest(proto::UnpublishDataTrackRequest),
}

/// Signal message received from the SFU handled by [`PubManager`].
#[derive(Debug, FromVariants)]
pub enum PubSignalInput {
    PublishResponse(proto::PublishDataTrackResponse),
    UnpublishResponse(proto::UnpublishDataTrackResponse),
    RequestResponse(proto::RequestResponse),
    SyncState(proto::SyncState),
}

impl DataTrackOptions {
    fn into_add_track_request(
        self,
        use_e2ee: bool,
        handle: TrackHandle,
    ) -> proto::PublishDataTrackRequest {
        let encryption = if self.disable_e2ee || !use_e2ee {
            proto::encryption::Type::None
        } else {
            proto::encryption::Type::Gcm
        };
        proto::PublishDataTrackRequest {
            pub_handle: handle.into(),
            name: self.name,
            encryption: encryption.into(),
        }
    }
}

impl From<proto::request_response::Reason> for PublishError {
    fn from(reason: proto::request_response::Reason) -> Self {
        use proto::request_response::Reason;
        // If new error cases are added in the future, consider if they should
        // be treated as internal errors or added to the public error enum.
        match reason {
            Reason::NotAllowed => PublishError::NotAllowed,
            Reason::DuplicateName => PublishError::DuplicateName,
            other => PublishError::Internal(anyhow!("SFU rejected: {:?}", other).into()),
        }
    }
}

impl TryInto<DataTrackInfo> for proto::PublishDataTrackResponse {
    type Error = InternalError;
    fn try_into(self) -> Result<DataTrackInfo, Self::Error> {
        let info = self.info.context("Missing info")?;
        info.try_into()
    }
}

impl From<PubSignalOutput> for proto::signal_request::Message {
    fn from(output: PubSignalOutput) -> Self {
        use proto::signal_request::Message;
        match output {
            PubSignalOutput::PublishRequest(req) => Message::PublishDataTrackRequest(req),
            PubSignalOutput::UnpublishRequest(req) => Message::UnpublishDataTrackRequest(req),
        }
    }
}
