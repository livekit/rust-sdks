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
//! Everything holding [`crate::server::data_track`]'s objects onto the protobuf path: the
//! handle bridges, the request handlers, the subscription handle, and the task that pumps
//! stream events.
//!
//! This whole file goes when the protobuf path does. What it leaves behind over there is
//! the `handle_id` field on each struct, and the `pub(crate)` these reach through.

use futures_util::StreamExt;
use livekit::data_track::{self as dt, DataTrackSubscribeError};
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, Notify};

use crate::server::data_track::{LocalDataTrack, RemoteDataTrack};
use crate::server::{FfiHandle, FfiServer};
use crate::{proto, FfiHandleId, FfiResult};

crate::migrate_from_ffi!(LocalDataTrack);

crate::migrate_from_ffi!(RemoteDataTrack);

impl LocalDataTrack {
    pub fn from_track(track: dt::LocalDataTrack) -> proto::OwnedLocalDataTrack {
        let info = track.info().clone();
        let track = Arc::new(Self { inner: track, handle_id: OnceLock::new() });
        proto::OwnedLocalDataTrack {
            handle: proto::FfiOwnedHandle { id: track.ffi_handle_id() },
            info: info.into(),
        }
    }

    pub fn is_published_ffi(
        &self,
        _request: proto::LocalDataTrackIsPublishedRequest,
    ) -> FfiResult<proto::LocalDataTrackIsPublishedResponse> {
        Ok(proto::LocalDataTrackIsPublishedResponse { is_published: self.is_published() })
    }

    pub fn unpublish_ffi(
        &self,
        _request: proto::LocalDataTrackUnpublishRequest,
    ) -> FfiResult<proto::LocalDataTrackUnpublishResponse> {
        self.unpublish();
        Ok(proto::LocalDataTrackUnpublishResponse::default())
    }

    pub fn try_push_ffi(
        &self,
        request: proto::LocalDataTrackTryPushRequest,
    ) -> FfiResult<proto::LocalDataTrackTryPushResponse> {
        let frame: dt::DataTrackFrame = request.frame.into();
        let error = self.inner.try_push(frame).err().map(Into::into);
        Ok(proto::LocalDataTrackTryPushResponse { error })
    }
}

impl RemoteDataTrack {
    pub fn from_track(track: dt::RemoteDataTrack) -> proto::OwnedRemoteDataTrack {
        let info = track.info().clone();
        let publisher_identity = track.publisher_identity().to_string();
        let track = Arc::new(Self { inner: track, handle_id: OnceLock::new() });
        proto::OwnedRemoteDataTrack {
            handle: proto::FfiOwnedHandle { id: track.ffi_handle_id() },
            publisher_identity,
            info: info.into(),
        }
    }

    pub fn subscribe_ffi(
        &self,
        server: &'static FfiServer,
        request: proto::SubscribeDataTrackRequest,
    ) -> FfiResult<proto::SubscribeDataTrackResponse> {
        let handle_id = server.next_id();
        let (drop_tx, drop_rx) = oneshot::channel();
        let notify_read = Arc::new(Notify::new());
        let eos_event = Arc::new(OnceLock::new());

        let subscription = DataTrackSubscription {
            notify_read: notify_read.clone(),
            eos_event: eos_event.clone(),
            drop_tx,
        };
        server.store_handle(handle_id, subscription);

        let task = SubscriptionTask { server, handle_id, notify_read, eos_event, drop_rx };
        let task_handle =
            server.async_runtime.spawn(task.run(self.inner.clone(), request.options.into()));
        server.watch_panic(task_handle);

        let stream =
            proto::OwnedDataTrackStream { handle: proto::FfiOwnedHandle { id: handle_id } };
        Ok(proto::SubscribeDataTrackResponse { stream })
    }

    pub fn is_published_ffi(
        &self,
        _request: proto::RemoteDataTrackIsPublishedRequest,
    ) -> FfiResult<proto::RemoteDataTrackIsPublishedResponse> {
        Ok(proto::RemoteDataTrackIsPublishedResponse { is_published: self.is_published() })
    }

    pub fn set_pipeline_options_ffi(
        &self,
        request: proto::RemoteDataTrackSetPipelineOptionsRequest,
    ) -> FfiResult<proto::RemoteDataTrackSetPipelineOptionsResponse> {
        self.inner.set_pipeline_options(request.options.into());
        Ok(proto::RemoteDataTrackSetPipelineOptionsResponse::default())
    }
}

/// What the protobuf path hands out in place of [`crate::server::data_track::DataTrackStream`].
///
/// Its `subscribe` returns a handle before the subscription has resolved, so there is no
/// stream to wrap yet: [`SubscriptionTask`] owns that and pushes frames out as events,
/// while this holds the pair that drives it. Over uniffi the stream is owned directly and
/// none of this is needed.
pub struct DataTrackSubscription {
    notify_read: Arc<Notify>,
    // Populated with the end-of-stream event once the stream has ended.
    eos_event: Arc<OnceLock<proto::DataTrackStreamEos>>,
    /// Used to drop the associated task when self is dropped
    #[allow(dead_code)]
    drop_tx: oneshot::Sender<()>,
}

impl FfiHandle for DataTrackSubscription {}

impl DataTrackSubscription {
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
    async fn run(mut self, track: dt::RemoteDataTrack, options: dt::DataTrackSubscribeOptions) {
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
        track: dt::RemoteDataTrack,
        options: dt::DataTrackSubscribeOptions,
    ) -> Option<dt::DataTrackStream> {
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

    async fn next_frame(&mut self, stream: &mut dt::DataTrackStream) -> Option<dt::DataTrackFrame> {
        tokio::select! {
            _ = &mut self.drop_rx => None,
            _ = self.notify_read.notified() => stream.next().await,
        }
    }

    fn send_frame(&self, frame: dt::DataTrackFrame) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FFI_SERVER;

    fn store_subscription() -> (FfiHandleId, Arc<Notify>, Arc<OnceLock<proto::DataTrackStreamEos>>)
    {
        let handle_id = FFI_SERVER.next_id();
        let (drop_tx, _drop_rx) = oneshot::channel();
        let notify_read = Arc::new(Notify::new());
        let eos_event = Arc::new(OnceLock::new());
        FFI_SERVER.store_handle(
            handle_id,
            DataTrackSubscription {
                notify_read: notify_read.clone(),
                eos_event: eos_event.clone(),
                drop_tx,
            },
        );
        (handle_id, notify_read, eos_event)
    }

    /// The handle map is typed by downcast, so `requests::on_data_track_stream_read` naming
    /// a different type than `subscribe_ffi` stored would only fail at runtime.
    #[serial_test::parallel]
    #[test]
    fn a_stored_subscription_comes_back_under_the_type_requests_asks_for() {
        let (handle_id, notify_read, _eos) = store_subscription();
        let notified = notify_read.notified();

        let subscription = FFI_SERVER
            .retrieve_handle::<DataTrackSubscription>(handle_id)
            .expect("subscription handle not found");
        let response = subscription.read(proto::DataTrackStreamReadRequest::default());

        assert!(response.eos_event.is_none(), "a live subscription reports no end-of-stream");
        assert!(
            futures_util::FutureExt::now_or_never(notified).is_some(),
            "read wakes the subscription task"
        );
    }

    /// Once the task has ended, `read` must answer from the stored event rather than wake a
    /// task that is no longer there.
    #[serial_test::parallel]
    #[test]
    fn a_finished_subscription_replays_its_end_of_stream() {
        let (handle_id, notify_read, eos_event) = store_subscription();
        eos_event.set(proto::DataTrackStreamEos { error: None }).expect("eos set twice");
        let notified = notify_read.notified();

        let subscription = FFI_SERVER
            .retrieve_handle::<DataTrackSubscription>(handle_id)
            .expect("subscription handle not found");
        let response = subscription.read(proto::DataTrackStreamReadRequest::default());

        assert!(response.eos_event.is_some(), "the stored end-of-stream is replayed");
        assert!(
            futures_util::FutureExt::now_or_never(notified).is_none(),
            "a finished subscription is not woken again"
        );
    }
}
