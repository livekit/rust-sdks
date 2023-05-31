use futures::stream::Stream;
use tokio::sync::mpsc;
use cxx::{SharedPtr, UniquePtr};
use webrtc_sys::{frame_transformer as sys_ft, encoded_video_frame::ffi::EncodedVideoFrame};

use crate::prelude::RtpReceiver;

pub struct NativeEncodedFrameStream {
    native_transfomer: SharedPtr<sys_ft::ffi::AdaptedNativeFrameTransformer>,
    // _observer: Box<VideoTrackObserver>,
    // video_track: RtcVideoTrack,
    // frame_rx: mpsc::UnboundedReceiver<EncodedFrame>,
}

impl NativeEncodedFrameStream {
    pub fn new(rtp_receiver: &RtpReceiver) -> Self {
        let mut observer = Box::new(VideoTrackEncodedFramesObserver { });
        let mut native_transfomer = unsafe {
            sys_ft::ffi::new_adapted_frame_transformer(Box::new(sys_ft::EncodedFrameSinkWrapper::new(
                &mut *observer,
            )))
        };

        rtp_receiver.set_depacketizer_to_decoder_frame_transformer(native_transfomer.clone());

        Self {
            native_transfomer: native_transfomer
        }
    }
}

// impl Stream for NativeEncodedFrameStream {
//     type Item = EncodedFrame;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
//         self.frame_rx.poll_recv(cx)
//     }
// }

struct VideoTrackEncodedFramesObserver {
    // frame_tx: mpsc::UnboundedSender<EncodedFrame>,
}

impl sys_ft::EncodedFrameSink for VideoTrackEncodedFramesObserver {
    // To be called when Transform happens
    fn on_encoded_frame(&self, frame: UniquePtr<EncodedVideoFrame>) {
        println!("VideoTrackEncodedFramesObserver::on_encoded_frame");
        println!("is_key_frame? {}", frame.is_key_frame());
        // TODO: send using frame_tx
    }
}
