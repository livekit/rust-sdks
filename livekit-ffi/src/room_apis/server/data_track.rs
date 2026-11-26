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

use super::{FfiHandle, FfiServer};
use crate::{proto, FfiHandleId};
use futures_util::StreamExt;
use livekit::data_track::{
    self as lk, DataTrackFrame, DataTrackStream, DataTrackSubscribeError, DataTrackSubscribeOptions,
};
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, Notify};

/// The shared uniffi objects. The handle map stores tracks as `Arc`s of these, so that
/// `from_ffi_handle_id` can hand the same track to the uniffi side.
pub use crate::data_track::{local::LocalDataTrack, remote::RemoteDataTrack};

pub struct FfiDataTrackStream {
    notify_read: Arc<Notify>,
    // Populated with the end-of-stream event once the stream has ended.
    eos_event: Arc<OnceLock<proto::DataTrackStreamEos>>,
    /// Used to drop the associated task when self is dropped
    #[allow(dead_code)]
    drop_tx: oneshot::Sender<()>,
}

impl FfiHandle for FfiDataTrackStream {}

impl FfiDataTrackStream {
    /// Subscribes to `track`, storing the stream in the handle map. The subscription itself is set
    /// up asynchronously; a failure arrives as the end-of-stream event.
    pub fn subscribe(
        server: &'static FfiServer,
        track: lk::RemoteDataTrack,
        options: DataTrackSubscribeOptions,
    ) -> proto::OwnedDataTrackStream {
        let handle_id = server.next_id();
        let (drop_tx, drop_rx) = oneshot::channel();
        let notify_read = Arc::new(Notify::new());
        let eos_event = Arc::new(OnceLock::new());

        let stream = FfiDataTrackStream {
            notify_read: notify_read.clone(),
            eos_event: eos_event.clone(),
            drop_tx,
        };
        server.store_handle(handle_id, stream);

        let task = SubscriptionTask { server, handle_id, notify_read, eos_event, drop_rx };
        let task_handle = server.async_runtime.spawn(task.run(track, options));
        server.watch_panic(task_handle);

        proto::OwnedDataTrackStream { handle: proto::FfiOwnedHandle { id: handle_id } }
    }

    pub fn read(
        &self,
        _request: proto::DataTrackStreamReadRequest,
    ) -> proto::DataTrackStreamReadResponse {
        let eos_event = self.eos_event.get().cloned();
        if eos_event.is_none() {
            self.notify_read.notify_one();
        }
        proto::DataTrackStreamReadResponse { eos_event }
    }
}

struct SubscriptionTask {
    server: &'static FfiServer,
    handle_id: FfiHandleId,
    notify_read: Arc<Notify>,
    eos_event: Arc<OnceLock<proto::DataTrackStreamEos>>,
    drop_rx: oneshot::Receiver<()>,
}

impl SubscriptionTask {
    async fn run(mut self, track: lk::RemoteDataTrack, options: DataTrackSubscribeOptions) {
        let Some(mut stream) = self.wait_for_subscription(track, options).await else {
            return;
        };
        while let Some(frame) = self.next_frame(&mut stream).await {
            self.send_frame(frame);
        }
        self.send_eos(None);
    }

    async fn wait_for_subscription(
        &mut self,
        track: lk::RemoteDataTrack,
        options: DataTrackSubscribeOptions,
    ) -> Option<DataTrackStream> {
        tokio::select! {
            _ = &mut self.drop_rx => None,
            result = track.subscribe_with_options(options) => match result {
                Ok(stream) => Some(stream),
                Err(err) => {
                    self.send_eos(Some(err));
                    None
                }
            },
        }
    }

    async fn next_frame(&mut self, stream: &mut DataTrackStream) -> Option<DataTrackFrame> {
        tokio::select! {
            _ = &mut self.drop_rx => None,
            _ = self.notify_read.notified() => stream.next().await,
        }
    }

    fn send_frame(&self, frame: DataTrackFrame) {
        let event = proto::DataTrackStreamEvent {
            stream_handle: self.handle_id,
            detail: Some(proto::DataTrackStreamFrameReceived { frame: frame.into() }.into()),
        };
        let _ = self.server.send_event(event.into());
    }

    fn send_eos(&self, error: Option<DataTrackSubscribeError>) {
        let eos_event = proto::DataTrackStreamEos { error: error.map(Into::into) };
        let _ = self.eos_event.set(eos_event.clone()); // Store for read request
        let event = proto::DataTrackStreamEvent {
            stream_handle: self.handle_id,
            detail: Some(eos_event.into()),
        };
        let _ = self.server.send_event(event.into());
    }
}
