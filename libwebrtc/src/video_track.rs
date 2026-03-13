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

use std::fmt::Debug;

use crate::{
    imp::video_track as imp_vt,
    media_stream_track::{media_stream_track, RtcTrackState},
};

#[cfg(not(target_arch = "wasm32"))]
use crate::native::user_timestamp::UserTimestampHandler;

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) handle: imp_vt::RtcVideoTrack,
}

impl RtcVideoTrack {
    media_stream_track!();

    /// Set the user timestamp handler for this track.
    ///
    /// When set, any `NativeVideoStream` created from this track will
    /// automatically use this handler to populate `user_timestamp_us`
    /// on each decoded frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_user_timestamp_handler(&self, handler: UserTimestampHandler) {
        self.handle.set_user_timestamp_handler(handler);
    }

    /// Get the user timestamp handler, if one has been set.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn user_timestamp_handler(&self) -> Option<UserTimestampHandler> {
        self.handle.user_timestamp_handler()
    }
}

impl Debug for RtcVideoTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcVideoTrack")
            .field("id", &self.id())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}
