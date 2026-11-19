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
use crate::native::packet_trailer::PacketTrailerHandler;

/// What a video track carries, as a hint to the encoder about whether to favour resolution or
/// frame rate when bandwidth runs short.
///
/// Mirrors [`MediaStreamTrack.contentHint`](https://www.w3.org/TR/mst-content-hint/) for video:
/// [`Detailed`](Self::Detailed) and [`Text`](Self::Text) keep resolution, [`Fluid`](Self::Fluid)
/// keeps frame rate. The hint only has an effect once the track is attached to a sender.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ContentHint {
    /// No hint; libwebrtc decides from the video source.
    #[default]
    None,
    /// Motion video: prefer frame rate over resolution.
    Fluid,
    /// Fine detail such as a diagram: prefer resolution over frame rate.
    Detailed,
    /// Text on a screen: prefer resolution over frame rate.
    Text,
}

#[derive(Clone)]
pub struct RtcVideoTrack {
    pub(crate) handle: imp_vt::RtcVideoTrack,
}

impl RtcVideoTrack {
    media_stream_track!();

    /// Set the packet trailer handler for this track.
    ///
    /// When set, any `NativeVideoStream` created from this track will
    /// automatically use this handler to populate `user_timestamp`
    /// on each decoded frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        self.handle.set_packet_trailer_handler(handler);
    }

    /// Get the packet trailer handler, if one has been set.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn packet_trailer_handler(&self) -> Option<PacketTrailerHandler> {
        self.handle.packet_trailer_handler()
    }

    /// Returns the content hint set on this track.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn content_hint(&self) -> ContentHint {
        self.handle.content_hint()
    }

    /// Sets the content hint, which the encoder uses once the track is sent.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_content_hint(&self, hint: ContentHint) {
        self.handle.set_content_hint(hint);
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
