use crate::imp::encoded_video_frame_stream as stream_imp;
// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {

    use crate::encoded_video_frame::EncodedVideoFrame;
    use crate::prelude::RtpReceiver;
    use super::stream_imp;
    use futures::stream::Stream;
    use webrtc_sys::encoded_video_frame::ffi::EncodedVideoFrame as sys_ef;
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use cxx::UniquePtr;

    pub struct NativeEncodedVideoFrameStream {
        pub(crate) handle: stream_imp::NativeEncodedVideoFrameStream,
    }

    impl Debug for NativeEncodedVideoFrameStream {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeEncodedVideoFrameStream")
                // .field("track", &self.track())
                .finish()
        }
    }

    impl NativeEncodedVideoFrameStream {
        pub fn new(rtp_receiver: &RtpReceiver) -> Self {
            Self {
                handle: stream_imp::NativeEncodedVideoFrameStream::new(rtp_receiver),
            }
        }

        pub fn frame_transformed(&mut self, frame: EncodedVideoFrame) {
            self.handle.frame_transformed(frame);
        }

        pub fn close(&mut self) {
            self.handle.close();
        }
    }

    impl Stream for NativeEncodedVideoFrameStream {
        type Item = EncodedVideoFrame;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.get_mut().handle).poll_next(cx)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
