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
use futures_util::{Stream, StreamExt};
use livekit::data_track::{DataTrackFrame, LocalDataTrack, RemoteDataTrack};
use tokio::sync::oneshot;

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
        let async_id = server.resolve_async_id(request.request_async_id);

        let handle = server.async_runtime.spawn(async move {
            let result = match self.inner.subscribe().await {
                Ok(stream) => proto::subscribe_data_track_callback::Result::Subscription(
                    FfiDataTrackSubscription::from_stream(server, stream),
                ),
                Err(err) => proto::subscribe_data_track_callback::Result::Error(err.to_string()),
            };
            let event = proto::SubscribeDataTrackCallback { async_id, result: Some(result) };
            let _ = server.send_event(event.into());
        });
        server.watch_panic(handle);

        Ok(proto::SubscribeDataTrackResponse { async_id })
    }
}

pub struct FfiDataTrackSubscription {
    #[allow(dead_code)]
    drop_tx: oneshot::Sender<()>, // Used to drop the associated task when self is dropped
}

impl FfiHandle for FfiDataTrackSubscription {}

impl FfiDataTrackSubscription {
    fn from_stream(
        server: &'static FfiServer,
        stream: impl Stream<Item = DataTrackFrame> + Send + 'static,
    ) -> proto::OwnedDataTrackSubscription {
        let (drop_tx, drop_rx) = oneshot::channel();
        let handle_id = server.next_id();

        let subscription = Self { drop_tx };
        server.store_handle(handle_id, subscription);

        let task_handle = server
            .async_runtime
            .spawn(data_track_subscription_task(server, handle_id, stream, drop_rx));
        server.watch_panic(task_handle);

        proto::OwnedDataTrackSubscription { handle: proto::FfiOwnedHandle { id: handle_id } }
    }
}

async fn data_track_subscription_task(
    server: &'static FfiServer,
    subscription_handle: FfiHandleId,
    stream: impl Stream<Item = DataTrackFrame> + Send + 'static,
    mut drop_rx: oneshot::Receiver<()>,
) {
    tokio::pin!(stream);
    loop {
        tokio::select! {
            _ = &mut drop_rx => break,
            Some(frame) = stream.next() => {
                let event = proto::DataTrackSubscriptionEvent {
                    subscription_handle,
                    detail: Some(proto::DataTrackSubscriptionFrameReceived { frame: frame.into() }.into()),
                };
                let _ = server.send_event(event.into());
            }
        }
    }
    let event = proto::DataTrackSubscriptionEvent {
        subscription_handle,
        detail: Some(proto::DataTrackSubscriptionEos::default().into()),
    };
    let _ = server.send_event(event.into());
}
