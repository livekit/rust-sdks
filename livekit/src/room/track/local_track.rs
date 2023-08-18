// Copyright 2023 LiveKit, Inc.
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

use super::track_dispatch;
use crate::prelude::*;
use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use livekit_webrtc::prelude::*;

#[derive(Clone, Debug)]
pub enum LocalTrack {
    Audio(LocalAudioTrack),
    Video(LocalVideoTrack),
}

impl LocalTrack {
    track_dispatch!([Audio, Video]);

    enum_dispatch!(
       [Audio, Video];
        pub fn mute(self: &Self) -> ();
        pub fn unmute(self: &Self) -> ();
    );

    pub fn rtc_track(&self) -> MediaStreamTrack {
        match self {
            Self::Audio(track) => track.rtc_track().into(),
            Self::Video(track) => track.rtc_track().into(),
        }
    }
}

impl From<LocalTrack> for Track {
    fn from(track: LocalTrack) -> Self {
        match track {
            LocalTrack::Audio(track) => Self::LocalAudio(track),
            LocalTrack::Video(track) => Self::LocalVideo(track),
        }
    }
}

impl TryFrom<Track> for LocalTrack {
    type Error = &'static str;

    fn try_from(track: Track) -> Result<Self, Self::Error> {
        match track {
            Track::LocalAudio(track) => Ok(Self::Audio(track)),
            Track::LocalVideo(track) => Ok(Self::Video(track)),
            _ => Err("not a local track"),
        }
    }
}
