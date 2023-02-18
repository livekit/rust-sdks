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

    extern "C++" {
        include!("livekit/video_frame.h");
        include!("livekit/helper.h");

        type VideoFrame = crate::video_frame::ffi::VideoFrame;
        type VideoTrackPtr = crate::helper::ffi::VideoTrackPtr;
        type AudioTrackPtr = crate::helper::ffi::AudioTrackPtr;
    }

    unsafe extern "C++" {
        include!("livekit/media_stream.h");

        type NativeVideoFrameSink;
        type MediaStreamTrack;
        type MediaStream;
        type AudioTrack;
        type VideoTrack;
        type AdaptedVideoTrackSource;

        fn id(self: &MediaStream) -> String;
        fn get_audio_tracks(self: &MediaStream) -> Vec<AudioTrackPtr>;
        fn get_video_tracks(self: &MediaStream) -> Vec<VideoTrackPtr>;
        fn find_audio_track(self: &MediaStream, track_id: String) -> SharedPtr<AudioTrack>;
        fn find_video_track(self: &MediaStream, track_id: String) -> SharedPtr<VideoTrack>;
        fn add_audio_track(self: &MediaStream, audio_track: SharedPtr<AudioTrack>) -> bool;
        fn add_video_track(self: &MediaStream, video_track: SharedPtr<VideoTrack>) -> bool;
        fn remove_audio_track(self: &MediaStream, audio_track: SharedPtr<AudioTrack>) -> bool;
        fn remove_video_track(self: &MediaStream, video_track: SharedPtr<VideoTrack>) -> bool;

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

        fn video_to_media(track: SharedPtr<VideoTrack>) -> SharedPtr<MediaStreamTrack>;
        fn audio_to_media(track: SharedPtr<AudioTrack>) -> SharedPtr<MediaStreamTrack>;
        fn media_to_video(track: SharedPtr<MediaStreamTrack>) -> SharedPtr<VideoTrack>;
        fn media_to_audio(track: SharedPtr<MediaStreamTrack>) -> SharedPtr<AudioTrack>;

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
