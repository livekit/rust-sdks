use crate::imp::video_sink as sink_imp;

// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::sink_imp;
    use crate::media_stream::VideoTrack;
    use crate::video_frame::{BoxVideoFrame};
    use std::fmt::Debug;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    pub struct NativeVideoSink {
        pub(crate) handle: sink_imp::NativeVideoSink,
    }

    impl Debug for NativeVideoSink {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSink")
                .field("track", &self.track())
                .finish()
        }
    }

    impl NativeVideoSink {
        pub fn new(video_track: VideoTrack) -> Self {
            Self {
                handle: sink_imp::NativeVideoSink::new(video_track),
            }
        }

        pub fn track(&self) -> VideoTrack {
            self.handle.track()
        }

        pub fn register_observer(&self) -> mpsc::UnboundedReceiver<Arc<BoxVideoFrame>> {
            self.handle.register_observer()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
