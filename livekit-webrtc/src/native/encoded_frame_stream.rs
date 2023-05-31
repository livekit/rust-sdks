use webrtc_sys::{frame_transformer as sys_ft, encoded_video_frame::ffi::EncodedVideoFrame};
use futures::stream::Stream;
use tokio::sync::mpsc;
use cxx::{SharedPtr, UniquePtr};
use crate::prelude::RtpReceiver;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct NativeEncodedFrameStream {
    native_transfomer: SharedPtr<sys_ft::ffi::AdaptedNativeFrameTransformer>,
    // _observer: Box<VideoTrackObserver>,
    // video_track: RtcVideoTrack,
    frame_rx: mpsc::UnboundedReceiver<UniquePtr<EncodedVideoFrame>>,
}

impl NativeEncodedFrameStream {
    pub fn new(rtp_receiver: &RtpReceiver) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let mut observer = Box::new(VideoTrackEncodedFramesObserver { frame_tx });
        let mut native_transfomer = unsafe {
            sys_ft::ffi::new_adapted_frame_transformer(Box::new(sys_ft::EncodedFrameSinkWrapper::new(
                &mut *observer,
            )))
        };

        rtp_receiver.set_depacketizer_to_decoder_frame_transformer(native_transfomer.clone());

        Self {
            native_transfomer: native_transfomer,
            frame_rx
        }
    }

    pub fn close(&mut self) {
        self.frame_rx.close();
        // unsafe {
        //     sys_ms::ffi::media_to_video(self.video_track.sys_handle())
        //         .remove_sink(self.native_observer.pin_mut());
        // }
    }
}

impl Stream for NativeEncodedFrameStream {
    type Item = UniquePtr<EncodedVideoFrame>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_rx.poll_recv(cx)
    }
}

struct VideoTrackEncodedFramesObserver {
    frame_tx: mpsc::UnboundedSender<UniquePtr<EncodedVideoFrame>>,
}

impl sys_ft::EncodedFrameSink for VideoTrackEncodedFramesObserver {
    // To be called when Transform happens
    fn on_encoded_frame(&self, frame: UniquePtr<EncodedVideoFrame>) {
        println!("VideoTrackEncodedFramesObserver::on_encoded_frame");
        println!("is_key_frame? {}", frame.is_key_frame());
        // TODO: send using frame_tx
        let _ = self.frame_tx.send(frame);
    }
}
