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

use std::{fmt::Debug, sync::Arc, time::Duration};

use libwebrtc::{native::packet_trailer::PacketTrailerHandler, prelude::*, stats::RtcStats};
use livekit_protocol as proto;

use super::{remote_track, TrackInner};
use crate::prelude::*;

#[derive(Clone)]
pub struct RemoteVideoTrack {
    inner: Arc<TrackInner>,
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

impl RemoteVideoTrack {
    pub(crate) fn new(sid: TrackSid, name: String, rtc_track: RtcVideoTrack) -> Self {
        Self {
            inner: Arc::new(super::new_inner(
                sid,
                name,
                TrackKind::Video,
                MediaStreamTrack::Video(rtc_track),
            )),
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
    pub fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.rtc_track().packet_trailer_handler()
    }

    /// Internal: set the handler that extracts packet trailers for this track.
    ///
    /// The handler is stored on the underlying `RtcVideoTrack`, so any
    /// `NativeVideoStream` created from this track will automatically
    /// pick it up — no manual wiring required.
    pub(crate) fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.rtc_track().set_packet_trailer_handler(handler);
    }

    pub async fn get_stats(&self) -> RoomResult<Vec<RtcStats>> {
        super::remote_track::get_stats(&self.inner).await
    }

    /// Requests a lower bound for the remote video's receiver playout delay.
    ///
    /// This is the only receiver playout-latency control currently exposed by
    /// the bound native WebRTC API. It is a best-effort hint, not a hard
    /// maximum or target, and actual end-to-end latency should be validated
    /// with inbound video stats.
    ///
    /// Passing `None` clears the override. Passing `Some(Duration::ZERO)`
    /// requests the lowest allowed playout floor without adding extra delay.
    pub fn set_minimum_playout_delay(&self, delay: Option<Duration>) -> RoomResult<()> {
        let Some(transceiver) = self.transceiver() else {
            return Err(RoomError::Internal("no transceiver found for track".into()));
        };

        transceiver.receiver().set_jitter_buffer_minimum_delay(delay);
        Ok(())
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
