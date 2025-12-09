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

use crate::{
    audio_track::RtcAudioTrack,
    sys::{self, lkMediaStream},
    video_track::RtcVideoTrack,
};

#[derive(Clone)]
pub struct MediaStream {
    pub(crate) ffi: sys::RefCounted<lkMediaStream>,
}

impl MediaStream {
    pub fn id(&self) -> String {
        unsafe {
            let len = sys::lkMediaStreamGetIdLength(self.ffi.as_ptr());
            let mut buf = vec![0u8; len as usize + 1];
            sys::lkMediaStreamGetId(self.ffi.as_ptr(), buf.as_mut_ptr() as *mut i8, len);
            let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
            cstr.to_string_lossy().into_owned()
        }
    }

    pub fn audio_tracks(&self) -> Vec<RtcAudioTrack> {
        let lk_vec = unsafe { sys::lkMediaStreamGetAudioTracks(self.ffi.as_ptr()) };
        let track_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if track_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut tracks = Vec::new();
        for i in 0..track_ptrs.vec.len() as isize {
            tracks.push(RtcAudioTrack { ffi: track_ptrs.vec[i as usize].clone() });
        }
        tracks
    }

    pub fn video_tracks(&self) -> Vec<RtcVideoTrack> {
        let lk_vec = unsafe { sys::lkMediaStreamGetAudioTracks(self.ffi.as_ptr()) };
        let track_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if track_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut tracks = Vec::new();
        for i in 0..track_ptrs.vec.len() as isize {
            tracks.push(RtcVideoTrack { ffi: track_ptrs.vec[i as usize].clone() });
        }
        tracks
    }
}
