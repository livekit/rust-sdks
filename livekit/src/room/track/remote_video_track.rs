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

use std::{
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use libwebrtc::{
    native::packet_trailer::{
        self, PacketTrailerHandler, SubscribeTimingObserver as RtcSubscribeTimingObserver,
    },
    prelude::*,
    stats::RtcStats,
};
use livekit_protocol as proto;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream};

use super::{remote_track, TrackInner};
use crate::{prelude::*, rtc_engine::lk_runtime::LkRuntime};

pub use libwebrtc::native::packet_trailer::{SubscribeTimingEvent, SubscribeTimingStage};

const SUBSCRIBE_TIMING_BUFFER: usize = 256;

#[derive(Clone)]
pub struct RemoteVideoTrack {
    inner: Arc<TrackInner>,
    subscribe_timing_tx: Arc<Mutex<Option<broadcast::Sender<SubscribeTimingEvent>>>>,
    subscribe_timing_observer: Arc<Mutex<Option<RtcSubscribeTimingObserver>>>,
}

impl Debug for RemoteVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteVideoTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

/// A stream of native remote video subscribe-pipeline timing events.
pub struct SubscribeTimingEventStream {
    inner: BroadcastStream<SubscribeTimingEvent>,
}

impl Stream for SubscribeTimingEventStream {
    type Item = SubscribeTimingEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => return Poll::Ready(Some(event)),
                Poll::Ready(Some(Err(_))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl RemoteVideoTrack {
    pub(crate) fn new(sid: TrackSid, name: String, rtc_track: RtcVideoTrack) -> Self {
        Self {
            inner: Arc::new(super::new_inner(
                sid,
                name,
                TrackKind::Video,
                MediaStreamTrack::Video(rtc_track),
            )),
            subscribe_timing_tx: Arc::new(Mutex::new(None)),
            subscribe_timing_observer: Arc::new(Mutex::new(None)),
        }
    }

    pub fn sid(&self) -> TrackSid {
        self.inner.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.inner.info.read().source
    }

    pub fn stream_state(&self) -> StreamState {
        self.inner.info.read().stream_state
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.rtc_track.enabled()
    }

    pub fn enable(&self) {
        self.inner.rtc_track.set_enabled(true);
    }

    pub fn disable(&self) {
        self.inner.rtc_track.set_enabled(false);
    }

    pub fn is_muted(&self) -> bool {
        self.inner.info.read().muted
    }

    pub fn rtc_track(&self) -> RtcVideoTrack {
        if let MediaStreamTrack::Video(video) = self.inner.rtc_track.clone() {
            return video;
        }
        unreachable!();
    }

    pub fn is_remote(&self) -> bool {
        true
    }

    /// Returns a clone of the packet trailer handler, if one has been set.
    pub(crate) fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.rtc_track().packet_trailer_handler()
    }

    /// Returns a stream of native remote video subscribe-pipeline timing events.
    ///
    /// Multiple concurrent subscriptions are supported; each call returns an
    /// independent stream that begins with the next event observed after this
    /// call. Slow consumers will silently drop intermediate events when the
    /// internal buffer fills.
    ///
    /// The underlying transformer is allocated lazily on first call. Call this
    /// before constructing a
    /// [`NativeVideoStream`](crate::webrtc::video_stream::native::NativeVideoStream)
    /// so decoder-output timing can be wired into the stream automatically.
    pub fn subscribe_timing_events(&self) -> SubscribeTimingEventStream {
        let tx = {
            let mut subscribe_timing_tx = self.subscribe_timing_tx.lock();
            if let Some(tx) = subscribe_timing_tx.as_ref() {
                tx.clone()
            } else {
                let (tx, _) = broadcast::channel(SUBSCRIBE_TIMING_BUFFER);
                *subscribe_timing_tx = Some(tx.clone());
                tx
            }
        };

        let handler = self.ensure_subscribe_timing_handler();
        if let Some(handler) = handler {
            self.apply_subscribe_timing_observer(&handler);
        }

        SubscribeTimingEventStream { inner: BroadcastStream::new(tx.subscribe()) }
    }

    /// Sets a direct observer for native remote video subscribe-pipeline timing events.
    ///
    /// This is useful for latency-sensitive consumers that want to avoid the
    /// allocation and task wakeup overhead of [`Self::subscribe_timing_events`].
    /// Call this before constructing a
    /// [`NativeVideoStream`](crate::webrtc::video_stream::native::NativeVideoStream)
    /// so decoder-output timing can be wired into the stream automatically.
    pub fn set_subscribe_timing_observer(&self, observer: Option<RtcSubscribeTimingObserver>) {
        *self.subscribe_timing_observer.lock() = observer;

        let handler = self.ensure_subscribe_timing_handler();
        if let Some(handler) = handler {
            self.apply_subscribe_timing_observer(&handler);
        }
    }

    /// Internal: set the handler that extracts packet trailers for this track.
    ///
    /// The handler is stored on the underlying `RtcVideoTrack`, so any
    /// `NativeVideoStream` created from this track will automatically
    /// pick it up — no manual wiring required.
    pub(crate) fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.apply_subscribe_timing_observer(&handler);
        self.rtc_track().set_packet_trailer_handler(handler);
    }

    fn ensure_subscribe_timing_handler(&self) -> Option<PacketTrailerHandler> {
        if let Some(handler) = self.packet_trailer_handler() {
            return Some(handler);
        }

        let transceiver = self.transceiver()?;
        let handler = packet_trailer::create_receiver_handler(
            LkRuntime::instance().pc_factory(),
            &transceiver.receiver(),
        );
        self.set_packet_trailer_handler(handler.clone());
        Some(handler)
    }

    fn apply_subscribe_timing_observer(&self, handler: &PacketTrailerHandler) {
        let tx = self.subscribe_timing_tx.lock().clone();
        let direct_observer = self.subscribe_timing_observer.lock().clone();
        let observer = match (tx, direct_observer) {
            (None, None) => None,
            (tx, direct_observer) => Some(Arc::new(move |event: SubscribeTimingEvent| {
                if let Some(observer) = direct_observer.as_ref() {
                    observer(event);
                }
                if let Some(tx) = tx.as_ref() {
                    let _ = tx.send(event);
                }
            }) as RtcSubscribeTimingObserver),
        };
        handler.set_subscribe_timing_observer(observer);
    }

    pub async fn get_stats(&self) -> RoomResult<Vec<RtcStats>> {
        super::remote_track::get_stats(&self.inner).await
    }

    pub(crate) fn on_muted(&self, f: impl Fn(Track) + Send + 'static) {
        self.inner.events.lock().muted.replace(Box::new(f));
    }

    pub(crate) fn on_unmuted(&self, f: impl Fn(Track) + Send + 'static) {
        self.inner.events.lock().unmuted.replace(Box::new(f));
    }

    pub(crate) fn transceiver(&self) -> Option<RtpTransceiver> {
        self.inner.info.read().transceiver.clone()
    }

    pub(crate) fn set_transceiver(&self, transceiver: Option<RtpTransceiver>) {
        self.inner.info.write().transceiver = transceiver;
    }

    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        remote_track::update_info(&self.inner, &Track::RemoteVideo(self.clone()), info);
    }
}
