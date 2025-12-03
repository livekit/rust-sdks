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
    video_track::RtcVideoTrack,
    sys::{self, lkMediaStream},
};

#[derive(Clone)]
pub struct MediaStream {
  pub(crate) ffi : sys::RefCounted<lkMediaStream>,
}

impl MediaStream {
  pub fn id(&self) -> String {
    unsafe {
      let len = sys::lkMediaStreamGetIdLength(self.ffi.as_ptr());
      let mut buf = vec ![0u8; len as usize + 1];
      sys::lkMediaStreamGetId(self.ffi.as_ptr(), buf.as_mut_ptr() as * mut i8,
                              len);
      let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as* const i8);
      cstr.to_string_lossy().into_owned()
    }
  }

  pub fn audio_tracks(&self) -> Vec<RtcAudioTrack> {
    let mut track_counts : ::std::os::raw::c_int = 0;
    let lk_audio_tracks = unsafe{
        sys::lkMediaStreamGetAudioTracks(self.ffi.as_ptr(), &mut track_counts)};
    let mut tracks = Vec::new ();
    for
      i in 0..track_counts {
        tracks.push(RtcAudioTrack{
          ffi : unsafe{
              sys::RefCounted::from_raw(*lk_audio_tracks.add(i as usize))},
        });
      }
    tracks
  }

  pub fn video_tracks(&self) -> Vec<RtcVideoTrack> {
    let mut track_counts : ::std::os::raw::c_int = 0;
    let lk_video_tracks = unsafe{
        sys::lkMediaStreamGetVideoTracks(self.ffi.as_ptr(), &mut track_counts)};
    let mut tracks = Vec::new ();
    for
      i in 0..track_counts {
        tracks.push(RtcVideoTrack{
          ffi : unsafe{
              sys::RefCounted::from_raw(*lk_video_tracks.add(i as usize))},
        });
      }
    tracks
  }
}
