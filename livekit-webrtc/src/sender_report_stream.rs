use crate::imp::sender_report_stream as stream_imp;
// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {

    use crate::sender_report::SenderReport;
    use crate::prelude::RtpReceiver;
    use super::stream_imp;
    use futures::stream::Stream;
    use webrtc_sys::sender_report::ffi::SenderReport as sys_sr;
    use std::fmt::Debug;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use cxx::UniquePtr;

    pub struct NativeSenderReportStream {
        pub(crate) handle: stream_imp::NativeSenderReportStream,
    }

    impl Debug for NativeSenderReportStream {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.debug_struct("NativeSenderReportStream")
                // .field("track", &self.track())
                .finish()
        }
    }

    impl NativeSenderReportStream {
        pub fn new(rtp_receiver: &RtpReceiver) -> Self {
            Self {
                handle: stream_imp::NativeSenderReportStream::new(rtp_receiver),
            }
        }

        pub fn close(&mut self) {
            self.handle.close();
        }
    }

    impl Stream for NativeSenderReportStream {
        type Item = SenderReport;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.get_mut().handle).poll_next(cx)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
