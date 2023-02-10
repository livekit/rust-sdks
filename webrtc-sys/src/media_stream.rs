use crate::impl_thread_safety;
use crate::video_frame::ffi::VideoFrame;
use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
    #[repr(i32)]
    pub enum TrackState {
        Live,
        Ended,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum ContentHint {
        None,
        Fluid,
        Detailed,
        Text,
    }

    // -1 = optional
    #[derive(Debug)]
    pub struct VideoTrackSourceConstraints {
        pub min_fps: f64,
        pub max_fps: f64,
    }

    unsafe extern "C++" {
        include!("livekit/media_stream.h");
        include!("livekit/video_frame.h");

        type NativeVideoFrameSink;
        type MediaStreamTrack;
        type MediaStream;
        type AudioTrack;
        type VideoTrack;
        type VideoFrame = crate::video_frame::ffi::VideoFrame;
        type AdaptedVideoTrackSource;

        fn id(self: &MediaStream) -> String;

        fn kind(self: &MediaStreamTrack) -> String;
        fn id(self: &MediaStreamTrack) -> String;
        fn enabled(self: &MediaStreamTrack) -> bool;
        fn set_enabled(self: &MediaStreamTrack, enable: bool) -> bool;
        fn state(self: &MediaStreamTrack) -> TrackState;

        unsafe fn add_sink(self: &VideoTrack, sink: Pin<&mut NativeVideoFrameSink>);
        unsafe fn remove_sink(self: &VideoTrack, sink: Pin<&mut NativeVideoFrameSink>);

        fn set_should_receive(self: &VideoTrack, should_receive: bool);
        fn should_receive(self: &VideoTrack) -> bool;
        fn content_hint(self: &VideoTrack) -> ContentHint;
        fn set_content_hint(self: &VideoTrack, hint: ContentHint);

        fn create_native_video_frame_sink(
            observer: Box<VideoFrameSinkWrapper>,
        ) -> UniquePtr<NativeVideoFrameSink>;

        fn on_captured_frame(self: &AdaptedVideoTrackSource, frame: UniquePtr<VideoFrame>) -> bool;

        unsafe fn media_to_video(track: *const MediaStreamTrack) -> *const VideoTrack;
        unsafe fn media_to_audio(track: *const MediaStreamTrack) -> *const AudioTrack;

        fn _shared_media_stream_track() -> SharedPtr<MediaStreamTrack>;
        fn _shared_audio_track() -> SharedPtr<AudioTrack>;
        fn _shared_video_track() -> SharedPtr<VideoTrack>;
        fn _shared_media_stream() -> SharedPtr<MediaStream>;
    }

    extern "Rust" {
        type VideoFrameSinkWrapper;

        fn on_frame(self: &VideoFrameSinkWrapper, frame: UniquePtr<VideoFrame>);
        fn on_discarded_frame(self: &VideoFrameSinkWrapper);
        fn on_constraints_changed(
            self: &VideoFrameSinkWrapper,
            constraints: VideoTrackSourceConstraints,
        );
    }
}

impl_thread_safety!(ffi::MediaStreamTrack, Send + Sync);
impl_thread_safety!(ffi::MediaStream, Send + Sync);
impl_thread_safety!(ffi::AudioTrack, Send + Sync);
impl_thread_safety!(ffi::VideoTrack, Send + Sync);
impl_thread_safety!(ffi::NativeVideoFrameSink, Send + Sync);

pub trait VideoFrameSink: Send + Sync {
    fn on_frame(&self, frame: UniquePtr<VideoFrame>);
    fn on_discarded_frame(&self);
    fn on_constraints_changed(&self, constraints: ffi::VideoTrackSourceConstraints);
}

pub struct VideoFrameSinkWrapper {
    observer: *mut dyn VideoFrameSink,
}

impl VideoFrameSinkWrapper {
    /// # Safety
    /// VideoFrameSink must lives as long as VideoSinkInterfaceWrapper does
    pub unsafe fn new(observer: *mut dyn VideoFrameSink) -> Self {
        Self { observer }
    }

    fn on_frame(&self, frame: UniquePtr<VideoFrame>) {
        unsafe {
            (*self.observer).on_frame(frame);
        }
    }

    fn on_discarded_frame(&self) {
        unsafe {
            (*self.observer).on_discarded_frame();
        }
    }

    fn on_constraints_changed(&self, constraints: ffi::VideoTrackSourceConstraints) {
        unsafe {
            (*self.observer).on_constraints_changed(constraints);
        }
    }
}
