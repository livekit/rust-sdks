use crate::imp::encoded_audio_frame_stream as stream_imp;
// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {

    use crate::encoded_audio_frame::EncodedAudioFrame;
    use crate::prelude::RtpReceiver;
    use super::stream_imp;
    use futures::stream::Stream;
    use webrtc_sys::encoded_audio_frame::ffi::EncodedAudioFrame as sys_ef;
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use cxx::UniquePtr;

    pub struct NativeEncodedAudioFrameStream {
        pub(crate) handle: stream_imp::NativeEncodedAudioFrameStream,
    }

    impl Debug for NativeEncodedAudioFrameStream {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeEncodedAudioFrameStream")
                // .field("track", &self.track())
                .finish()
        }
    }

    impl NativeEncodedAudioFrameStream {
        pub fn new(rtp_receiver: &RtpReceiver) -> Self {
            Self {
                handle: stream_imp::NativeEncodedAudioFrameStream::new(rtp_receiver),
            }
        }

        pub fn frame_transformed(&mut self, frame: EncodedAudioFrame) {
            self.handle.frame_transformed(frame);
        }

        pub fn close(&mut self) {
            self.handle.close();
        }
    }

    impl Stream for NativeEncodedAudioFrameStream {
        type Item = EncodedAudioFrame;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.get_mut().handle).poll_next(cx)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
