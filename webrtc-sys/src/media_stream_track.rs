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

#[cfg(not(target_arch = "wasm32"))]
use crate::sys;
use crate::{
    audio_track::{self, RtcAudioTrack},
    enum_dispatch,
    sys::{lkMediaStreamTrackKind, lkRtcTrackState},
    video_track::{self, RtcVideoTrack},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtcTrackState {
    Live,
    Ended,
}

impl From<lkRtcTrackState> for RtcTrackState {
    fn from(state: lkRtcTrackState) -> Self {
        match state {
            lkRtcTrackState::LK_RTC_TRACK_STATE_LIVE => RtcTrackState::Live,
            lkRtcTrackState::LK_RTC_TRACK_STATE_ENDED => RtcTrackState::Ended,
        }
    }
}

#[derive(Debug, Clone)]
pub enum MediaStreamTrack {
    Video(RtcVideoTrack),
    Audio(RtcAudioTrack),
}

#[cfg(not(target_arch = "wasm32"))]
impl MediaStreamTrack {
    enum_dispatch!(
        [Video, Audio];
        pub(crate) fn ffi(self: &Self) -> sys::RefCounted<sys::lkMediaStreamTrack>;
    );
}

impl MediaStreamTrack {
    enum_dispatch!(
        [Video, Audio];
        pub fn id(self: &Self) -> String;
        pub fn enabled(self: &Self) -> bool;
        pub fn set_enabled(self: &Self, enabled: bool) -> bool;
        pub fn state(self: &Self) -> RtcTrackState;
    );
}

macro_rules! media_stream_track {
    () => {
        pub fn id(&self) -> String {
            unsafe {
                let len = sys::lkMediaStreamTrackGetIdLength(self.ffi.as_ptr());
                let mut buf = vec![0u8; len as usize + 1];
                sys::lkMediaStreamTrackGetId(self.ffi.as_ptr(), buf.as_mut_ptr() as *mut i8, len);
                let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
                cstr.to_string_lossy().into_owned()
            }
        }

        pub fn enabled(&self) -> bool {
            unsafe { sys::lkMediaStreamTrackIsEnabled(self.ffi.as_ptr()) }
        }

        pub fn set_enabled(&self, enabled: bool) -> bool {
            unsafe { sys::lkMediaStreamTrackSetEnabled(self.ffi.as_ptr(), enabled) }
            enabled
        }

        pub fn state(&self) -> RtcTrackState {
            unsafe { sys::lkMediaStreamTrackGetState(self.ffi.as_ptr()).into() }
        }

        #[cfg(not(target_arch = "wasm32"))]
        pub(crate) fn ffi(&self) -> sys::RefCounted<sys::lkMediaStreamTrack> {
            self.ffi.clone()
        }
    };
}

pub(crate) use media_stream_track;

impl From<RtcAudioTrack> for MediaStreamTrack {
    fn from(track: RtcAudioTrack) -> Self {
        Self::Audio(track)
    }
}

impl From<RtcVideoTrack> for MediaStreamTrack {
    fn from(track: RtcVideoTrack) -> Self {
        Self::Video(track)
    }
}

pub fn new_media_stream_track(
    sys_handle: sys::RefCounted<sys::lkMediaStreamTrack>,
) -> MediaStreamTrack {
    let kind = unsafe { sys::lkMediaStreamTrackGetKind(sys_handle.as_ptr()) };
    if kind == lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_AUDIO {
        MediaStreamTrack::Audio(audio_track::RtcAudioTrack { ffi: sys_handle })
    } else if kind == lkMediaStreamTrackKind::LK_MEDIA_STREAM_TRACK_KIND_VIDEO {
        MediaStreamTrack::Video(video_track::RtcVideoTrack { ffi: sys_handle })
    } else {
        panic!("unknown track kind")
    }
}
