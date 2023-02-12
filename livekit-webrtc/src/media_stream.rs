use cxx::{SharedPtr, UniquePtr};
use livekit_utils::enum_dispatch;
use std::fmt::{Debug, Formatter};
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
    pub(crate) fn new(cxx_handle: SharedPtr<sys_ms::ffi::MediaStreamTrack>) -> Self {
        if cxx_handle.kind() == MEDIA_TYPE_VIDEO {
            Self::Video(VideoTrack::new(cxx_handle))
        } else {
            Self::Audio(AudioTrack::new(cxx_handle))
        }
    }

    // TODO(theomonnom): enum_dispatch with visibility support?
    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_ms::ffi::MediaStreamTrack> {
        match self {
            Self::Video(video) => video.cxx_handle(),
            Self::Audio(audio) => audio.cxx_handle(),
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
    cxx_handle: SharedPtr<sys_ms::ffi::MediaStreamTrack>,
}

impl AudioTrack {
    fn new(cxx_handle: SharedPtr<sys_ms::ffi::MediaStreamTrack>) -> Arc<Self> {
        Arc::new(Self { cxx_handle })
    }

    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_ms::ffi::MediaStreamTrack> {
        self.cxx_handle.clone()
    }
}

pub struct VideoTrack {
    cxx_handle: SharedPtr<sys_ms::ffi::MediaStreamTrack>,
    observer: Box<InternalVideoTrackSink>,

    // Keep alive for c++
    native_observer: UniquePtr<sys_ms::ffi::NativeVideoFrameSink>,
}

impl VideoTrack {
    fn new(cxx_handle: SharedPtr<sys_ms::ffi::MediaStreamTrack>) -> Arc<Self> {
        let mut observer = Box::new(InternalVideoTrackSink::default());

        let mut track = unsafe {
            Self {
                cxx_handle,
                native_observer: {
                    sys_ms::ffi::create_native_video_frame_sink(Box::new(
                        sys_ms::VideoFrameSinkWrapper::new(&mut *observer),
                    ))
                },
                observer,
            }
        };

        unsafe {
            (*track.video_handle()).add_sink(track.native_observer.pin_mut());
        }

        Arc::new(track)
    }

    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_ms::ffi::MediaStreamTrack> {
        self.cxx_handle.clone()
    }

    fn video_handle(&self) -> *const sys_ms::ffi::VideoTrack {
        unsafe { sys_ms::ffi::media_to_video(&*self.cxx_handle) }
    }

    pub fn set_should_receive(&self, should_receive: bool) {
        unsafe { (*self.video_handle()).set_should_receive(should_receive) }
    }

    pub fn set_content_hint(&self, hint: ContentHint) {
        unsafe { (*self.video_handle()).set_content_hint(hint) }
    }

    pub fn should_receive(&self) -> bool {
        unsafe { (*self.video_handle()).should_receive() }
    }

    pub fn content_hint(&self) -> ContentHint {
        unsafe { (*self.video_handle()).content_hint() }
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
            (*self.video_handle()).remove_sink(self.native_observer.pin_mut());
        }
    }
}

macro_rules! impl_media_stream_track_trait {
    ($x:ty) => {
        impl MediaStreamTrackTrait for $x {
            fn kind(&self) -> String {
                self.cxx_handle.kind()
            }

            fn id(&self) -> String {
                self.cxx_handle.id()
            }

            fn enabled(&self) -> bool {
                self.cxx_handle.enabled()
            }

            fn set_enabled(&self, enabled: bool) -> bool {
                self.cxx_handle.set_enabled(enabled)
            }

            fn state(&self) -> TrackState {
                self.cxx_handle.state()
            }
        }
    };
}

impl_media_stream_track_trait!(VideoTrack);
impl_media_stream_track_trait!(AudioTrack);

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

pub struct MediaStream {
    cxx_handle: SharedPtr<sys_ms::ffi::MediaStream>,
}

impl Debug for MediaStream {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("MediaStream")
            .field("id", &self.id())
            .finish()
    }
}

impl MediaStream {
    pub(crate) fn new(cxx_handle: SharedPtr<sys_ms::ffi::MediaStream>) -> Self {
        Self { cxx_handle }
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }
}
