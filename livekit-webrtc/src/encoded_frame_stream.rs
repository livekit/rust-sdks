use crate::imp::encoded_frame_stream as stream_imp;
// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {

    use crate::prelude::RtpReceiver;

    use super::stream_imp;
    use futures::stream::Stream;
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct NativeEncodedFrameStream {
        pub(crate) handle: stream_imp::NativeEncodedFrameStream,
    }

    impl Debug for NativeEncodedFrameStream {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeEncodedFrameStream")
                // .field("track", &self.track())
                .finish()
        }
    }

    impl NativeEncodedFrameStream {
        pub fn new(rtp_receiver: &RtpReceiver) -> Self {
        // pub fn new(video_track: RtcVideoTrack) -> Self {
            Self {
                handle: stream_imp::NativeEncodedFrameStream::new(rtp_receiver),
            }
        }

        // pub fn track(&self) -> RtcVideoTrack {
        //     self.handle.track()
        // }

        pub fn close(&mut self) {
            // self.handle.close();
        }
    }

    // impl Stream for NativeEncodedFrameStream {
    //     type Item = BoxVideoFrame;

    //     fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
    //         Pin::new(&mut self.get_mut().handle).poll_next(cx)
    //     }
    // }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
