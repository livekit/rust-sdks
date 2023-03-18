use crate::imp::video_stream as stream_imp;

// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use super::stream_imp;
    use crate::media_stream::RtcVideoTrack;
    use crate::video_frame::BoxVideoFrame;
    use futures::stream::Stream;
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct NativeVideoStream {
        pub(crate) handle: stream_imp::NativeVideoStream,
    }

    impl Debug for NativeVideoStream {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoStream")
                .field("track", &self.track())
                .finish()
        }
    }

    impl NativeVideoStream {
        pub fn new(video_track: RtcVideoTrack) -> Self {
            Self {
                handle: stream_imp::NativeVideoStream::new(video_track),
            }
        }

        pub fn track(&self) -> RtcVideoTrack {
            self.handle.track()
        }

        pub fn close(&mut self) {
            self.handle.close();
        }
    }

    impl Stream for NativeVideoStream {
        type Item = BoxVideoFrame;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.get_mut().handle).poll_next(cx)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
