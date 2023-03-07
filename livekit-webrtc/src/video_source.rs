use crate::imp::video_source as vs_imp;

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::vs_imp;
    use crate::video_frame::{VideoFrame, VideoFrameBuffer};
    use std::fmt::{Debug, Formatter};

    #[derive(Default)]
    pub struct NativeVideoSource {
        pub(crate) handle: vs_imp::NativeVideoSource,
    }

    impl Debug for NativeVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("VideoSource").finish()
        }
    }

    impl NativeVideoSource {
        pub fn capture_frame<T: VideoFrameBuffer>(&self, frame: VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
