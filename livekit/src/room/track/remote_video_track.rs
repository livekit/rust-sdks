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

use std::{fmt::Debug, sync::Arc};

use libwebrtc::{native::user_timestamp::UserTimestampHandler, prelude::*, stats::RtcStats};
use livekit_protocol as proto;
use parking_lot::Mutex;

use super::{remote_track, TrackInner};
use crate::prelude::*;

#[derive(Clone)]
pub struct RemoteVideoTrack {
    inner: Arc<TrackInner>,
    user_timestamp_handler: Arc<Mutex<Option<UserTimestampHandler>>>,
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
            user_timestamp_handler: Arc::new(Mutex::new(None)),
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

    /// Returns the last parsed user timestamp (in microseconds) for this
    /// remote video track, if the user timestamp transformer is enabled and
    /// a timestamp has been received.
    pub fn last_user_timestamp(&self) -> Option<i64> {
        self.user_timestamp_handler
            .lock()
            .as_ref()
            .and_then(|h| h.last_user_timestamp())
    }

    /// Returns a clone of the user timestamp handler, if one has been set.
    ///
    /// This can be passed to a `NativeVideoStream` via
    /// `set_user_timestamp_handler` so that each frame's
    /// `user_timestamp_us` field is populated automatically.
    pub fn user_timestamp_handler(&self) -> Option<UserTimestampHandler> {
        self.user_timestamp_handler.lock().clone()
    }

    /// Internal: set the handler that extracts user timestamps for this track.
    pub(crate) fn set_user_timestamp_handler(&self, handler: UserTimestampHandler) {
        self.user_timestamp_handler.lock().replace(handler);
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
