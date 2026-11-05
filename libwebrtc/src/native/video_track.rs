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
use super::packet_trailer::PacketTrailerHandler;
use crate::{media_stream_track::RtcTrackState, video_track::ContentHint};

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) sys_handle: SharedPtr<sys_vt::ffi::VideoTrack>,
    packet_trailer_handler: Arc<Mutex<Option<PacketTrailerHandler>>>,
}

impl RtcVideoTrack {
    impl_media_stream_track!(video_to_media);

    pub(crate) fn new(sys_handle: SharedPtr<sys_vt::ffi::VideoTrack>) -> Self {
        Self { sys_handle, packet_trailer_handler: Arc::new(Mutex::new(None)) }
    }

    pub fn sys_handle(&self) -> SharedPtr<sys_vt::ffi::MediaStreamTrack> {
        video_to_media(self.sys_handle.clone())
    }

    /// Set the packet trailer handler for this track.
    ///
    /// When set, any `NativeVideoStream` created from this track will
    /// automatically use this handler to populate `user_timestamp`
    /// on each decoded frame.
    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.packet_trailer_handler.lock().replace(handler);
    }

    /// Get the packet trailer handler, if one has been set.
    pub fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.packet_trailer_handler.lock().clone()
    }

    pub fn content_hint(&self) -> ContentHint {
        self.sys_handle.content_hint().into()
    }

    pub fn set_content_hint(&self, hint: ContentHint) {
        self.sys_handle.set_content_hint(hint.into());
    }
}

impl From<sys_vt::ffi::ContentHint> for ContentHint {
    fn from(hint: sys_vt::ffi::ContentHint) -> Self {
        match hint {
            sys_vt::ffi::ContentHint::Fluid => Self::Fluid,
            sys_vt::ffi::ContentHint::Detailed => Self::Detailed,
            sys_vt::ffi::ContentHint::Text => Self::Text,
            // `None` and any value the C++ enum gains later.
            _ => Self::None,
        }
    }
}

impl From<ContentHint> for sys_vt::ffi::ContentHint {
    fn from(hint: ContentHint) -> Self {
        match hint {
            ContentHint::None => Self::None,
            ContentHint::Fluid => Self::Fluid,
            ContentHint::Detailed => Self::Detailed,
            ContentHint::Text => Self::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hint_round_trips_through_the_sys_enum() {
        for hint in
            [ContentHint::None, ContentHint::Fluid, ContentHint::Detailed, ContentHint::Text]
        {
            assert_eq!(ContentHint::from(sys_vt::ffi::ContentHint::from(hint)), hint);
        }
    }
}
