use webrtc_sys::{frame_transformer as sys_ft};
use futures::stream::Stream;
use tokio::sync::mpsc;
use cxx::{SharedPtr, UniquePtr};
use crate::encoded_audio_frame::EncodedAudioFrame;
use webrtc_sys::encoded_audio_frame::ffi::EncodedAudioFrame as sys_eaf;
use webrtc_sys::encoded_video_frame::ffi::EncodedVideoFrame as sys_evf;
use crate::prelude::RtpReceiver;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct NativeEncodedAudioFrameStream {
    native_transfomer: SharedPtr<sys_ft::ffi::AdaptedNativeFrameTransformer>,
    _observer: Box<AudioTrackEncodedAudioFramesObserver>,
    frame_rx: mpsc::UnboundedReceiver<EncodedAudioFrame>,
}

impl NativeEncodedAudioFrameStream {
    pub fn new(rtp_receiver: &RtpReceiver) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let mut observer = Box::new(AudioTrackEncodedAudioFramesObserver { frame_tx });
        let mut native_transfomer = unsafe {
            sys_ft::ffi::new_adapted_frame_transformer(Box::new(sys_ft::EncodedFrameSinkWrapper::new(
                &mut *observer,
            )), false)
        };

        rtp_receiver.set_depacketizer_to_decoder_frame_transformer(native_transfomer.clone());

        Self {
            native_transfomer: native_transfomer,
            _observer: observer,
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

impl Drop for NativeEncodedAudioFrameStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeEncodedAudioFrameStream {
    type Item = EncodedAudioFrame;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_rx.poll_recv(cx)
    }
}

struct AudioTrackEncodedAudioFramesObserver {
    frame_tx: mpsc::UnboundedSender<EncodedAudioFrame>,
}

impl sys_ft::EncodedFrameSink for AudioTrackEncodedAudioFramesObserver {
    // To be called when Transform happens
    fn on_encoded_audio_frame(&self, frame: UniquePtr<sys_eaf>) {
        // println!("AudioTrackEncodedAudioFramesObserver::on_encoded_audio_frame");
        let encoded_frame = EncodedAudioFrame::new(frame);
        let _ = self.frame_tx.send(encoded_frame);
    }

    fn on_encoded_video_frame(&self, frame: UniquePtr<sys_evf>) {
        // println!("AudioTrackEncodedAudioFramesObserver::on_encoded_video_frame");
        panic!("Receiving video frames in an audio receiver");
    }
}
