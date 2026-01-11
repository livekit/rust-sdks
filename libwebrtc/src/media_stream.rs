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
    impl_thread_safety,
    sys::{self, lkMediaStream},
    video_track::RtcVideoTrack,
};
use std::fmt::Debug;

#[derive(Clone)]
pub struct MediaStream {
    pub(crate) ffi: sys::RefCounted<lkMediaStream>,
}

impl MediaStream {
    pub fn id(&self) -> String {
        unsafe {
            let str_ptr = sys::lkMediaStreamGetId(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }

    pub fn audio_tracks(&self) -> Vec<RtcAudioTrack> {
        let lk_vec = unsafe { sys::lkMediaStreamGetAudioTracks(self.ffi.as_ptr()) };
        let item_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if item_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut items = Vec::new();
        for i in 0..item_ptrs.vec.len() as isize {
            items.push(RtcAudioTrack { ffi: item_ptrs.vec[i as usize].clone() });
        }
        items
    }

    pub fn video_tracks(&self) -> Vec<RtcVideoTrack> {
        let lk_vec = unsafe { sys::lkMediaStreamGetAudioTracks(self.ffi.as_ptr()) };
        let item_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if item_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut items = Vec::new();
        for i in 0..item_ptrs.vec.len() as isize {
            items.push(RtcVideoTrack { ffi: item_ptrs.vec[i as usize].clone() });
        }
        items
    }
}

impl Debug for MediaStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaStream")
            .field("id", &self.id())
            .field("audio_tracks", &self.audio_tracks())
            .field("video_tracks", &self.video_tracks())
            .finish()
    }
}

impl_thread_safety!(MediaStream, Send + Sync);
