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
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::*;
    use crate::video_frame::{VideoFrame, VideoFrameBuffer};
    use std::fmt::{Debug, Formatter};

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
            Self {
                handle: vs_imp::NativeVideoSource::new(resolution),
            }
        }

        pub fn capture_frame<T: AsRef<dyn VideoFrameBuffer>>(&self, frame: &VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }

        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
