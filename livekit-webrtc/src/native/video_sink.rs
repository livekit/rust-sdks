use super::video_frame::new_video_frame_buffer;
use crate::media_stream::VideoTrack;
use crate::video_frame::{BoxVideoFrame, VideoFrame};
use cxx::UniquePtr;
use livekit_utils::observer::Dispatcher;
use std::sync::Arc;
use tokio::sync::mpsc;
use webrtc_sys::media_stream as sys_ms;

#[allow(dead_code)] // Keep the C++ handles alive
pub struct NativeVideoSink {
    native_observer: UniquePtr<sys_ms::ffi::NativeVideoFrameSink>,
    observer: Box<VideoTrackSink>,
    video_track: VideoTrack,
}

impl NativeVideoSink {
    pub fn new(video_track: VideoTrack) -> Self {
        let mut observer = Box::new(VideoTrackSink::default());
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
            observer,
            video_track,
        }
    }

    pub fn track(&self) -> VideoTrack {
        self.video_track.clone()
    }

    pub fn register_observer(&self) -> mpsc::UnboundedReceiver<Arc<BoxVideoFrame>> {
        self.observer.dispatcher.register()
    }
}

#[derive(Default)]
struct VideoTrackSink {
    dispatcher: Dispatcher<Arc<BoxVideoFrame>>,
}

impl sys_ms::VideoFrameSink for VideoTrackSink {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        self.dispatcher.dispatch(&Arc::new(VideoFrame {
            id: frame.id(),
            rotation: frame.rotation().into(),
            buffer: new_video_frame_buffer(unsafe { frame.video_frame_buffer() }),
        }));
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: sys_ms::ffi::VideoTrackSourceConstraints) {}
}
