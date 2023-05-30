// use futures::stream::Stream;
// use tokio::sync::mpsc;

// pub struct NativeFrameTransformerStream {
//     native_observer: UniquePtr<sys_ms::ffi::NativeVideoFrameSink>,
//     _observer: Box<VideoTrackObserver>,
//     video_track: RtcVideoTrack,
//     frame_rx: mpsc::UnboundedReceiver<EncodedFrame>,
// }

// impl Stream for NativeVideoStream {
//     type Item = EncodedFrame;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
//         self.frame_rx.poll_recv(cx)
//     }
// }

// struct VideoTrackObserver {
//     frame_tx: mpsc::UnboundedSender<EncodedFrame>,
// }

// impl sys_ms::VideoFrameSink for VideoTrackObserver {
//     // To be called when Transform happens
//     fn on_encoded_frame(&self) {
//         // TODO: send using frame_tx
//     }
// }
