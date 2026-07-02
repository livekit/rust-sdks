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
    fmt::{Debug, Display},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use libwebrtc::{
    native::packet_trailer::{
        self, PacketTrailerHandler, PublishTimingObserver as RtcPublishTimingObserver,
    },
    prelude::*,
    stats::RtcStats,
};
use livekit_protocol as proto;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream};

use super::TrackInner;
use crate::{prelude::*, rtc_engine::lk_runtime::LkRuntime};

pub use libwebrtc::native::packet_trailer::{PublishTimingEvent, PublishTimingStage};

const PUBLISH_TIMING_BUFFER: usize = 256;

/// A simulcast layer currently configured on a local video publisher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishingLayer {
    /// RTP stream identifier for this simulcast layer.
    pub rid: String,
    /// Video quality represented by this simulcast layer.
    pub quality: PublishingLayerQuality,
    /// Whether this simulcast layer is currently being published.
    pub active: bool,
}

/// Video quality for a publishing layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishingLayerQuality {
    /// Low video quality.
    Low,
    /// Medium video quality.
    Medium,
    /// High video quality.
    High,
    /// The layer is disabled and not being encoded/sent.
    ///
    /// Mirrors `VideoQuality::OFF` from the LiveKit protocol. In practice this
    /// is not emitted by [`LocalVideoTrack::publishing_layers`] (a disabled
    /// layer is instead reflected by [`PublishingLayer::active`] being
    /// `false`); it exists to fully model the protocol enum and for conversions
    /// to/from it.
    Off,
}

impl From<proto::VideoQuality> for PublishingLayerQuality {
    fn from(quality: proto::VideoQuality) -> Self {
        match quality {
            proto::VideoQuality::Low => Self::Low,
            proto::VideoQuality::Medium => Self::Medium,
            proto::VideoQuality::High => Self::High,
            proto::VideoQuality::Off => Self::Off,
        }
    }
}

impl From<PublishingLayerQuality> for proto::VideoQuality {
    fn from(quality: PublishingLayerQuality) -> Self {
        match quality {
            PublishingLayerQuality::Low => Self::Low,
            PublishingLayerQuality::Medium => Self::Medium,
            PublishingLayerQuality::High => Self::High,
            PublishingLayerQuality::Off => Self::Off,
        }
    }
}

impl Display for PublishingLayerQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Off => write!(f, "off"),
        }
    }
}

#[derive(Clone)]
pub struct LocalVideoTrack {
    inner: Arc<TrackInner>,
    source: RtcVideoSource,
    packet_trailer_handler: Arc<Mutex<Option<PacketTrailerHandler>>>,
    publish_timing_tx: Arc<Mutex<Option<broadcast::Sender<PublishTimingEvent>>>>,
}

impl Debug for LocalVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalVideoTrack")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("source", &self.source())
            .finish()
    }
}

/// A stream of native local video publish-pipeline timing events.
pub struct PublishTimingEventStream {
    inner: BroadcastStream<PublishTimingEvent>,
}

impl Stream for PublishTimingEventStream {
    type Item = PublishTimingEvent;

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

impl LocalVideoTrack {
    pub fn new(name: String, rtc_track: RtcVideoTrack, source: RtcVideoSource) -> Self {
        Self {
            inner: Arc::new(super::new_inner(
                "TR_unknown".to_owned().try_into().unwrap(),
                name,
                TrackKind::Video,
                MediaStreamTrack::Video(rtc_track),
            )),
            source,
            packet_trailer_handler: Arc::new(Mutex::new(None)),
            publish_timing_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn create_video_track(name: &str, source: RtcVideoSource) -> LocalVideoTrack {
        let rtc_track = match source.clone() {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(native_source) => {
                use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
                LkRuntime::instance()
                    .pc_factory()
                    .create_video_track(&libwebrtc::native::create_random_uuid(), native_source)
            }
            _ => panic!("unsupported video source"),
        };

        Self::new(name.to_string(), rtc_track, source)
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

    pub fn mute(&self) {
        super::set_muted(&self.inner, &Track::LocalVideo(self.clone()), true);
    }

    pub fn unmute(&self) {
        super::set_muted(&self.inner, &Track::LocalVideo(self.clone()), false);
    }

    pub fn rtc_track(&self) -> RtcVideoTrack {
        if let MediaStreamTrack::Video(video) = self.inner.rtc_track.clone() {
            return video;
        }
        unreachable!();
    }

    pub fn is_remote(&self) -> bool {
        false
    }

    pub fn rtc_source(&self) -> RtcVideoSource {
        self.source.clone()
    }

    /// Returns a stream of native local video publish-pipeline timing events.
    ///
    /// Multiple concurrent subscriptions are supported; each call returns an
    /// independent stream that begins with the next event observed after this
    /// call. Slow consumers will silently drop intermediate events when the
    /// internal buffer fills.
    ///
    /// The underlying transformer is allocated lazily on first call. If invoked
    /// before the track is published, instrumentation is enabled at publish time.
    pub fn publish_timing_events(&self) -> PublishTimingEventStream {
        let tx = {
            let mut publish_timing_tx = self.publish_timing_tx.lock();
            if let Some(tx) = publish_timing_tx.as_ref() {
                tx.clone()
            } else {
                let (tx, _) = broadcast::channel(PUBLISH_TIMING_BUFFER);
                *publish_timing_tx = Some(tx.clone());
                tx
            }
        };

        let handler = self.ensure_publish_timing_handler();
        if let Some(handler) = handler {
            self.apply_publish_timing_observer(&handler);
        }

        PublishTimingEventStream { inner: BroadcastStream::new(tx.subscribe()) }
    }

    /// Returns the packet trailer handler associated with this track, if any.
    /// When present on the sender side, callers can store per-frame user
    /// timestamps which will be embedded into encoded frames.
    pub(crate) fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.packet_trailer_handler.lock().clone()
    }

    pub(crate) fn has_publish_timing_subscribers(&self) -> bool {
        self.publish_timing_tx.lock().is_some()
    }

    /// Internal: set the packet trailer handler used for this track.
    pub(crate) fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.apply_publish_timing_observer(&handler);
        *self.packet_trailer_handler.lock() = Some(handler);
    }

    fn ensure_publish_timing_handler(&self) -> Option<PacketTrailerHandler> {
        if let Some(handler) = self.packet_trailer_handler.lock().clone() {
            return Some(handler);
        }

        let transceiver = self.transceiver()?;
        let handler = packet_trailer::create_sender_handler(
            LkRuntime::instance().pc_factory(),
            &transceiver.sender(),
        );
        handler.set_enabled(false);
        self.set_packet_trailer_handler(handler.clone());

        #[cfg(not(target_arch = "wasm32"))]
        if let RtcVideoSource::Native(ref native_source) = self.rtc_source() {
            native_source.set_packet_trailer_handler(handler.clone());
        }

        Some(handler)
    }

    fn apply_publish_timing_observer(&self, handler: &PacketTrailerHandler) {
        let tx = self.publish_timing_tx.lock().clone();
        let observer = tx.map(|tx| {
            Arc::new(move |event: PublishTimingEvent| {
                let _ = tx.send(event);
            }) as RtcPublishTimingObserver
        });
        handler.set_publish_timing_observer(observer);
    }

    pub async fn get_stats(&self) -> RoomResult<Vec<RtcStats>> {
        super::local_track::get_stats(&self.inner).await
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
        super::update_info(&self.inner, &Track::LocalVideo(self.clone()), info);
    }

    /// Returns a snapshot of each simulcast layer's RID, quality, and active state.
    /// Useful for diagnostics / HUD display.
    /// Returns an empty vec if no transceiver is set, or if the sender has no encodings yet.
    pub fn publishing_layers(&self) -> Vec<PublishingLayer> {
        let Some(transceiver) = self.transceiver() else {
            log::debug!("dynacast: no transceiver, returning empty layers");
            return Vec::new();
        };
        let params = transceiver.sender().parameters();
        params
            .encodings
            .iter()
            .map(|e| {
                let quality = crate::options::video_quality_for_rid_or_default(&e.rid);
                PublishingLayer { rid: e.rid.clone(), quality: quality.into(), active: e.active }
            })
            .collect()
    }

    /// Toggle simulcast encoding layers on/off based on subscriber demand.
    /// Used by dynacast: the SFU tells us which quality levels are needed,
    /// and we set `encoding.active` accordingly on the RTP sender.
    pub(crate) fn set_publishing_layers(
        &self,
        qualities: &[proto::SubscribedQuality],
    ) -> RoomResult<()> {
        let transceiver = self.transceiver().ok_or_else(|| {
            RoomError::Internal("cannot set publishing layers: no transceiver".into())
        })?;

        let sender = transceiver.sender();
        let mut params = sender.parameters();

        if params.encodings.is_empty() {
            log::debug!("dynacast: no sender encodings available, ignoring quality update");
            return Ok(());
        }

        let mut changed = false;
        for encoding in &mut params.encodings {
            // The SFU addresses layers by spatial index (0 = Low), so a
            // single rid-less encoding is addressed as Low, not High.
            let rid = if encoding.rid.is_empty() { "q" } else { encoding.rid.as_str() };
            let quality = crate::options::video_quality_for_rid_or_default(rid);

            // A quality missing from the update is left untouched.
            let Some(subscribed) = qualities.iter().find(|q| q.quality == quality as i32) else {
                continue;
            };

            if encoding.active != subscribed.enabled {
                changed = true;
                encoding.active = subscribed.enabled;
            }
        }

        let layers: Vec<String> = params
            .encodings
            .iter()
            .map(|e| {
                let quality = crate::options::video_quality_for_rid_or_default(&e.rid);
                let state = if e.active { "ON" } else { "off" };
                format!("{}({:?})={}", e.rid, quality, state)
            })
            .collect();

        if changed {
            sender.set_parameters(params).map_err(|e| {
                RoomError::Internal(format!("failed to set sender parameters: {}", e))
            })?;
            log::debug!("dynacast: layers changed -> [{}]", layers.join(", "));
        } else {
            log::debug!("dynacast: layers unchanged [{}]", layers.join(", "));
        }

        Ok(())
    }
}
