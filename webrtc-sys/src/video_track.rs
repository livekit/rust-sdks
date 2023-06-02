use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[repr(i32)]
    pub enum ContentHint {
        None,
        Fluid,
        Detailed,
        Text,
    }

    #[derive(Debug)]
    pub struct VideoTrackSourceConstraints {
        pub has_min_fps: bool,
        pub min_fps: f64,
        pub has_max_fps: bool,
        pub max_fps: f64,
    }

    extern "C++" {
        include!("livekit/video_frame.h");

        type VideoFrame = crate::video_frame::ffi::VideoFrame;
    }

    unsafe extern "C++" {
        include!("livekit/video_track.h");

        type VideoTrack;
        type NativeVideoSink;
        type VideoTrackSource;

        fn add_sink(self: &VideoTrack, sink: &SharedPtr<NativeVideoSink>);
        fn remove_sink(self: &VideoTrack, sink: &SharedPtr<NativeVideoSink>);
        fn set_should_receive(self: &VideoTrack, should_receive: bool);
        fn should_receive(self: &VideoTrack) -> bool;
        fn content_hint(self: &VideoTrack) -> ContentHint;
        fn set_content_hint(self: &VideoTrack, hint: ContentHint);
        fn new_native_video_sink(observer: Box<VideoSinkWrapper>) -> SharedPtr<NativeVideoSink>;

        fn on_captured_frame(self: &VideoTrackSource, frame: &UniquePtr<VideoFrame>) -> bool;
        fn new_video_track_source() -> SharedPtr<VideoTrackSource>;
    }

    extern "Rust" {
        type VideoSinkWrapper;

        fn on_frame(self: &VideoSinkWrapper, frame: UniquePtr<VideoFrame>);
        fn on_discarded_frame(self: &VideoSinkWrapper);
        fn on_constraints_changed(
            self: &VideoSinkWrapper,
            constraints: VideoTrackSourceConstraints,
        );
    }
}

impl_thread_safety!(ffi::VideoTrack, Send + Sync);
impl_thread_safety!(ffi::NativeVideoSink, Send + Sync);
impl_thread_safety!(ffi::VideoTrackSource, Send + Sync);

pub trait VideoSink: Send {
    fn on_frame(&self, frame: UniquePtr<VideoFrame>);
    fn on_discarded_frame(&self);
    fn on_constraints_changed(&self, constraints: ffi::VideoTrackSourceConstraints);
}

pub struct VideoSinkWrapper {
    observer: Box<dyn VideoSink>,
}

impl VideoSinkWrapper {
    pub fn new(observer: Box<dyn VideoSink>) -> Self {
        Self { observer }
    }

    fn on_frame(&self, frame: UniquePtr<VideoFrame>) {
        self.observer.on_frame(frame);
    }

    fn on_discarded_frame(&self) {
        self.observer.on_discarded_frame();
    }

    fn on_constraints_changed(&self, constraints: ffi::VideoTrackSourceConstraints) {
        self.observer.on_constraints_changed(constraints);
    }
}
