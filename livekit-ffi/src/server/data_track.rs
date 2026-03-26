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
use crate::{proto, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::data_track::{
    DataTrackFrame, DataTrackSubscribeOptions, DataTrackSubscription, LocalDataTrack,
    RemoteDataTrack,
};
use std::sync::Arc;
use tokio::sync::{oneshot, Notify};

/// FFI wrapper around [`LocalDataTrack`].
#[derive(Clone)]
pub struct FfiLocalDataTrack {
    pub handle_id: FfiHandleId,
    pub inner: LocalDataTrack,
}

/// FFI wrapper around [`RemoteDataTrack`].
#[derive(Clone)]
pub struct FfiRemoteDataTrack {
    pub handle_id: FfiHandleId,
    pub inner: RemoteDataTrack,
}

impl FfiHandle for FfiLocalDataTrack {}
impl FfiHandle for FfiRemoteDataTrack {}

impl FfiLocalDataTrack {
    pub fn from_track(
        server: &'static FfiServer,
        track: LocalDataTrack,
    ) -> proto::OwnedLocalDataTrack {
        let handle_id = server.next_id();
        let info = track.info().clone();
        let track = Self { handle_id, inner: track };
        server.store_handle(handle_id, track);
        proto::OwnedLocalDataTrack {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: info.into(),
        }
    }

    pub fn is_published(
        self,
        _server: &'static FfiServer,
        _request: proto::LocalDataTrackIsPublishedRequest,
    ) -> FfiResult<proto::LocalDataTrackIsPublishedResponse> {
        let is_published = self.inner.is_published();
        Ok(proto::LocalDataTrackIsPublishedResponse { is_published })
    }

    pub fn unpublish(
        self,
        _server: &'static FfiServer,
        _request: proto::LocalDataTrackUnpublishRequest,
    ) -> FfiResult<proto::LocalDataTrackUnpublishResponse> {
        self.inner.unpublish();
        Ok(proto::LocalDataTrackUnpublishResponse::default())
    }

    pub fn try_push(
        self,
        _server: &'static FfiServer,
        request: proto::LocalDataTrackTryPushRequest,
    ) -> FfiResult<proto::LocalDataTrackTryPushResponse> {
        let frame: DataTrackFrame = request.frame.into();
        let error: Option<String> = self.inner.try_push(frame).err().map(|err| err.to_string());
        Ok(proto::LocalDataTrackTryPushResponse { error })
    }
}

impl FfiRemoteDataTrack {
    pub fn from_track(
        server: &'static FfiServer,
        track: RemoteDataTrack,
    ) -> proto::OwnedRemoteDataTrack {
        let handle_id = server.next_id();
        let info = track.info().clone();
        let publisher_identity = track.publisher_identity().to_string();
        let track = Self { handle_id, inner: track };
        server.store_handle(handle_id, track);
        proto::OwnedRemoteDataTrack {
            handle: proto::FfiOwnedHandle { id: handle_id },
            publisher_identity,
            info: info.into(),
        }
    }

    pub fn is_published(
        self,
        _server: &'static FfiServer,
        _request: proto::RemoteDataTrackIsPublishedRequest,
    ) -> FfiResult<proto::RemoteDataTrackIsPublishedResponse> {
        let is_published = self.inner.is_published();
        Ok(proto::RemoteDataTrackIsPublishedResponse { is_published })
    }

    pub fn subscribe(
        self,
        server: &'static FfiServer,
        request: proto::SubscribeDataTrackRequest,
    ) -> FfiResult<proto::SubscribeDataTrackResponse> {
        let handle_id = server.next_id();
        let (drop_tx, drop_rx) = oneshot::channel();
        let notify_read = Arc::new(Notify::new());

        let subscription = FfiDataTrackSubscription { notify_read: notify_read.clone(), drop_tx };
        server.store_handle(handle_id, subscription);

        let task = SubscriptionTask { server, handle_id, notify_read, drop_rx };
        let task_handle =
            server.async_runtime.spawn(task.run(self.inner, request.options.into()));
        server.watch_panic(task_handle);

        let subscription = proto::OwnedDataTrackSubscription {
            handle: proto::FfiOwnedHandle { id: handle_id },
        };
        Ok(proto::SubscribeDataTrackResponse { subscription })
    }
}

pub struct FfiDataTrackSubscription {
    notify_read: Arc<Notify>,
    #[allow(dead_code)]
    drop_tx: oneshot::Sender<()>, // Used to drop the associated task when self is dropped
}

impl FfiHandle for FfiDataTrackSubscription {}

impl FfiDataTrackSubscription {
    pub fn read(
        &self,
        _request: proto::DataTrackSubscriptionReadRequest,
    ) -> proto::DataTrackSubscriptionReadResponse {
        self.notify_read.notify_one();
        proto::DataTrackSubscriptionReadResponse::default()
    }
}

struct SubscriptionTask {
    server: &'static FfiServer,
    handle_id: FfiHandleId,
    notify_read: Arc<Notify>,
    drop_rx: oneshot::Receiver<()>,
}

impl SubscriptionTask {
    async fn run(mut self, track: RemoteDataTrack, options: DataTrackSubscribeOptions) {
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
        track: RemoteDataTrack,
        options: DataTrackSubscribeOptions,
    ) -> Option<DataTrackSubscription> {
        tokio::select! {
            _ = &mut self.drop_rx => None,
            result = track.subscribe_with_options(options) => match result {
                Ok(stream) => Some(stream),
                Err(err) => {
                    self.send_eos(Some(err.to_string()));
                    None
                }
            },
        }
    }

    async fn next_frame(&mut self, stream: &mut DataTrackSubscription) -> Option<DataTrackFrame> {
        tokio::select! {
            _ = &mut self.drop_rx => None,
            _ = self.notify_read.notified() => stream.next().await,
        }
    }

    fn send_frame(&self, frame: DataTrackFrame) {
        let event = proto::DataTrackSubscriptionEvent {
            subscription_handle: self.handle_id,
            detail: Some(proto::DataTrackSubscriptionFrameReceived { frame: frame.into() }.into()),
        };
        let _ = self.server.send_event(event.into());
    }

    fn send_eos(&self, error: Option<String>) {
        let event = proto::DataTrackSubscriptionEvent {
            subscription_handle: self.handle_id,
            detail: Some(proto::DataTrackSubscriptionEos { error }.into()),
        };
        let _ = self.server.send_event(event.into());
    }
}
