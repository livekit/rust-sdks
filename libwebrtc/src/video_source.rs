// Copyright 2024-2025 LiveKit, Inc.
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

use livekit_protocol::enum_dispatch;

use crate::imp::video_source as vs_imp;

#[derive(Default, Debug, Clone)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcVideoSource {
    // TODO(theomonnom): Web video sources (eq. to tracks on browsers?)
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeVideoSource),
}

// TODO(theomonnom): Support enum dispatch with conditional compilation?
impl RtcVideoSource {
    enum_dispatch!(
        [Native];
        pub fn video_resolution(self: &Self) -> VideoResolution;
    );
    enum_dispatch!(
        [Native];
        pub fn set_is_screencast(self: &Self, is_screencast: bool);
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    use super::*;
    use crate::video_frame::{VideoBuffer, VideoFrame};

    #[derive(Clone)]
    pub struct NativeVideoSource {
        pub(crate) handle: vs_imp::NativeVideoSource,
    }

    impl Debug for NativeVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSource").finish()
        }
    }

    impl Default for NativeVideoSource {
        fn default() -> Self {
            Self::new(VideoResolution::default())
        }
    }

    impl NativeVideoSource {
        pub fn new(resolution: VideoResolution) -> Self {
            Self { handle: vs_imp::NativeVideoSource::new(resolution) }
        }

        pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }

        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }

        pub fn set_is_screencast(&self, is_screencast: bool) {
            self.handle.set_is_screencast(is_screencast)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
