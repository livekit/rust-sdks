use cxx::UniquePtr;
use livekit_utils::enum_dispatch;
use std::fmt::{Debug, Formatter};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use webrtc_sys::media_stream as sys_ms;
use webrtc_sys::MEDIA_TYPE_VIDEO;

pub use sys_ms::ffi::ContentHint;
pub use sys_ms::ffi::TrackState;

use crate::video_frame::VideoFrame;
use crate::video_frame_buffer::VideoFrameBuffer;

pub trait MediaStreamTrackTrait {
    fn kind(&self) -> String;
    fn id(&self) -> String;
    fn enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> bool;
    fn state(&self) -> TrackState;
}

#[derive(Clone)]
pub enum MediaStreamTrackHandle {
    Audio(Arc<AudioTrack>),
    Video(Arc<VideoTrack>),
}

impl MediaStreamTrackHandle {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::MediaStreamTrack>) -> Self {
        unsafe {
            if cxx_handle.kind() == MEDIA_TYPE_VIDEO {
                Self::Video(VideoTrack::new(UniquePtr::from_raw(
                    sys_ms::ffi::media_to_video(cxx_handle.into_raw())
                        as *mut sys_ms::ffi::VideoTrack,
                )))
            } else {
                Self::Audio(AudioTrack::new(UniquePtr::from_raw(
                    sys_ms::ffi::media_to_audio(cxx_handle.into_raw())
                        as *mut sys_ms::ffi::AudioTrack,
                )))
            }
        }
    }
}

impl Debug for MediaStreamTrackHandle {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("MediaStreamTrack")
            .field("id", &self.id())
            .field("kind", &self.kind())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
}

impl MediaStreamTrackTrait for MediaStreamTrackHandle {
    enum_dispatch!(
        [Audio, Video]
        fnc!(kind, &Self, [], String);
        fnc!(id, &Self, [], String);
        fnc!(enabled, &Self, [], bool);
        fnc!(state, &Self, [], TrackState);
        fnc!(set_enabled, &Self, [enabled: bool], bool);
    );
}

pub struct AudioTrack {
    cxx_handle: Mutex<UniquePtr<sys_ms::ffi::AudioTrack>>,
}

impl AudioTrack {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::AudioTrack>) -> Arc<Self> {
        Arc::new(Self {
            cxx_handle: Mutex::new(cxx_handle),
        })
    }
}

pub struct VideoTrack {
    cxx_handle: Mutex<UniquePtr<sys_ms::ffi::VideoTrack>>,
    observer: Box<InternalVideoTrackSink>,

    // Keep alive for c++
    native_observer: UniquePtr<sys_ms::ffi::NativeVideoFrameSink>,
}

macro_rules! impl_media_stream_track_trait {
    ($x:ty, $cast:ident) => {
        impl MediaStreamTrackTrait for $x {
            fn kind(&self) -> String {
                unsafe { (*sys_ms::ffi::$cast(&**self.cxx_handle.lock().unwrap())).kind() }
            }

            fn id(&self) -> String {
                unsafe { (*sys_ms::ffi::$cast(&**self.cxx_handle.lock().unwrap())).id() }
            }

            fn enabled(&self) -> bool {
                unsafe { (*sys_ms::ffi::$cast(&**self.cxx_handle.lock().unwrap())).enabled() }
            }

            fn set_enabled(&self, enabled: bool) -> bool {
                unsafe {
                    let media = sys_ms::ffi::$cast(&**self.cxx_handle.lock().unwrap())
                        as *mut sys_ms::ffi::MediaStreamTrack;

                    Pin::new_unchecked(&mut *media).set_enabled(enabled)
                }
            }

            fn state(&self) -> TrackState {
                unsafe { (*sys_ms::ffi::$cast(&**self.cxx_handle.lock().unwrap())).state() }
            }
        }
    };
}

impl_media_stream_track_trait!(VideoTrack, video_to_media);
impl_media_stream_track_trait!(AudioTrack, audio_to_media);

pub type OnFrameHandler = Box<dyn FnMut(VideoFrame, VideoFrameBuffer) + Send + Sync>;
pub type OnDiscardedFrameHandler = Box<dyn FnMut() + Send + Sync>;
pub type OnConstraintsChangedHandler = Box<dyn FnMut(VideoTrackSourceConstraints) + Send + Sync>;

#[derive(Default)]
struct InternalVideoTrackSink {
    on_frame_handler: Mutex<Option<OnFrameHandler>>,
    on_discarded_frame_handler: Mutex<Option<OnDiscardedFrameHandler>>,
    on_constraints_changed_handler: Mutex<Option<OnConstraintsChangedHandler>>,
}

pub struct VideoTrackSourceConstraints {
    pub min_fps: Option<f64>,
    pub max_fps: Option<f64>,
}

impl From<sys_ms::ffi::VideoTrackSourceConstraints> for VideoTrackSourceConstraints {
    fn from(cst: sys_ms::ffi::VideoTrackSourceConstraints) -> Self {
        Self {
            min_fps: (cst.min_fps != 1.0).then_some(cst.min_fps),
            max_fps: (cst.max_fps != 1.0).then_some(cst.max_fps),
        }
    }
}

impl sys_ms::VideoFrameSink for InternalVideoTrackSink {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        if let Some(cb) = self.on_frame_handler.lock().unwrap().as_mut() {
            let frame = VideoFrame::new(frame);
            let video_frame_buffer = unsafe { frame.video_frame_buffer() };
            cb(frame, video_frame_buffer);
        }
    }

    fn on_discarded_frame(&self) {
        if let Some(cb) = self.on_discarded_frame_handler.lock().unwrap().as_mut() {
            cb();
        }
    }

    fn on_constraints_changed(&self, constraints: sys_ms::ffi::VideoTrackSourceConstraints) {
        if let Some(cb) = self.on_constraints_changed_handler.lock().unwrap().as_mut() {
            cb(constraints.into());
        }
    }
}

impl VideoTrack {
    fn new(cxx_handle: UniquePtr<sys_ms::ffi::VideoTrack>) -> Arc<Self> {
        let mut observer = Box::new(InternalVideoTrackSink::default());

        let mut track = unsafe {
            Self {
                cxx_handle: Mutex::new(cxx_handle),
                native_observer: sys_ms::ffi::create_native_video_frame_sink(Box::new(
                    sys_ms::VideoFrameSinkWrapper::new(&mut *observer),
                )),
                observer,
            }
        };

        unsafe {
            track
                .cxx_handle
                .lock()
                .unwrap()
                .pin_mut()
                .add_sink(track.native_observer.pin_mut());
        }

        Arc::new(track)
    }

    pub fn set_should_receive(&self, should_receive: bool) {
        self.cxx_handle
            .lock()
            .unwrap()
            .pin_mut()
            .set_should_receive(should_receive)
    }

    pub fn set_content_hint(&self, hint: ContentHint) {
        self.cxx_handle
            .lock()
            .unwrap()
            .pin_mut()
            .set_content_hint(hint)
    }

    pub fn should_receive(&self) -> bool {
        self.cxx_handle.lock().unwrap().should_receive()
    }

    pub fn content_hint(&self) -> ContentHint {
        self.cxx_handle.lock().unwrap().content_hint()
    }

    pub fn on_frame(&self, handler: OnFrameHandler) {
        *self.observer.on_frame_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_discarded_frame(&self, handler: OnDiscardedFrameHandler) {
        *self.observer.on_discarded_frame_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_constraints_changed(&self, handler: OnConstraintsChangedHandler) {
        *self.observer.on_constraints_changed_handler.lock().unwrap() = Some(handler);
    }
}

impl Drop for VideoTrack {
    fn drop(&mut self) {
        unsafe {
            self.cxx_handle
                .lock()
                .unwrap()
                .pin_mut()
                .remove_sink(self.native_observer.pin_mut());
        }
    }
}

pub struct MediaStream {
    cxx_handle: UniquePtr<sys_ms::ffi::MediaStream>,
}

impl Debug for MediaStream {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("MediaStream")
            .field("id", &self.id())
            .finish()
    }
}

impl MediaStream {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::MediaStream>) -> Self {
        Self { cxx_handle }
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }
}
