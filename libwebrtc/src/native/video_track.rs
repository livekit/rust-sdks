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

use std::sync::Arc;

use cxx::SharedPtr;
use parking_lot::Mutex;
use sys_vt::ffi::video_to_media;
use webrtc_sys::video_track as sys_vt;

use super::media_stream_track::impl_media_stream_track;
use super::user_timestamp::UserTimestampHandler;
use crate::media_stream_track::RtcTrackState;

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) sys_handle: SharedPtr<sys_vt::ffi::VideoTrack>,
    user_timestamp_handler: Arc<Mutex<Option<UserTimestampHandler>>>,
}

impl RtcVideoTrack {
    impl_media_stream_track!(video_to_media);

    pub(crate) fn new(sys_handle: SharedPtr<sys_vt::ffi::VideoTrack>) -> Self {
        Self { sys_handle, user_timestamp_handler: Arc::new(Mutex::new(None)) }
    }

    pub fn sys_handle(&self) -> SharedPtr<sys_vt::ffi::MediaStreamTrack> {
        video_to_media(self.sys_handle.clone())
    }

    /// Set the user timestamp handler for this track.
    ///
    /// When set, any `NativeVideoStream` created from this track will
    /// automatically use this handler to populate `user_timestamp_us`
    /// on each decoded frame.
    pub fn set_user_timestamp_handler(&self, handler: UserTimestampHandler) {
        self.user_timestamp_handler.lock().replace(handler);
    }

    /// Get the user timestamp handler, if one has been set.
    pub fn user_timestamp_handler(&self) -> Option<UserTimestampHandler> {
        self.user_timestamp_handler.lock().clone()
    }
}
