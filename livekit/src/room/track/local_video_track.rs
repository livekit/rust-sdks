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

use super::TrackInner;
use crate::{prelude::*, rtc_engine::lk_runtime::LkRuntime};

#[derive(Clone)]
pub struct LocalVideoTrack {
    inner: Arc<TrackInner>,
    source: RtcVideoSource,
    user_timestamp_handler: Arc<Mutex<Option<UserTimestampHandler>>>,
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
            user_timestamp_handler: Arc::new(Mutex::new(None)),
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

    /// Returns the user timestamp handler associated with this track, if any.
    /// When present on the sender side, callers can store per-frame user
    /// timestamps which will be embedded into encoded frames.
    pub fn user_timestamp_handler(&self) -> Option<UserTimestampHandler> {
        self.user_timestamp_handler.lock().clone()
    }

    /// Internal: set the user timestamp handler used for this track.
    pub(crate) fn set_user_timestamp_handler(&self, handler: UserTimestampHandler) {
        *self.user_timestamp_handler.lock() = Some(handler);
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
}
