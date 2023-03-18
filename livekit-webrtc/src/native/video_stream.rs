use super::video_frame::new_video_frame_buffer;
use crate::media_stream::RtcVideoTrack;
use crate::video_frame::{BoxVideoFrame, VideoFrame};
use cxx::UniquePtr;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use webrtc_sys::media_stream as sys_ms;

pub struct NativeVideoStream {
    native_observer: UniquePtr<sys_ms::ffi::NativeVideoFrameSink>,
    _observer: Box<VideoTrackObserver>,
    video_track: RtcVideoTrack,
    frame_rx: mpsc::UnboundedReceiver<BoxVideoFrame>,
}

impl NativeVideoStream {
    pub fn new(video_track: RtcVideoTrack) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let mut observer = Box::new(VideoTrackObserver { frame_tx });
        let mut native_observer = unsafe {
            sys_ms::ffi::new_native_video_frame_sink(Box::new(sys_ms::VideoFrameSinkWrapper::new(
                &mut *observer,
            )))
        };

        unsafe {
            sys_ms::ffi::media_to_video(video_track.sys_handle())
                .add_sink(native_observer.pin_mut());
        }

        Self {
            native_observer,
            _observer: observer,
            video_track,
            frame_rx,
        }
    }

    pub fn track(&self) -> RtcVideoTrack {
        self.video_track.clone()
    }

    pub fn close(&mut self) {
        self.frame_rx.close();
        unsafe {
            sys_ms::ffi::media_to_video(self.video_track.sys_handle())
                .remove_sink(self.native_observer.pin_mut());
        }
    }
}

impl Drop for NativeVideoStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeVideoStream {
    type Item = BoxVideoFrame;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_rx.poll_recv(cx)
    }
}

struct VideoTrackObserver {
    frame_tx: mpsc::UnboundedSender<BoxVideoFrame>,
}

impl sys_ms::VideoFrameSink for VideoTrackObserver {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        let _ = self.frame_tx.send(VideoFrame {
            rotation: frame.rotation().into(),
            timestamp: frame.timestamp_us(),
            buffer: new_video_frame_buffer(unsafe { frame.video_frame_buffer() }),
        });
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: sys_ms::ffi::VideoTrackSourceConstraints) {}
}
