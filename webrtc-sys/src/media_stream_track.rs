use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[repr(i32)]
    pub enum TrackState {
        Live,
        Ended,
    }

    unsafe extern "C++" {
        include!("livekit/media_stream_track.h");

        type MediaStreamTrack;

        fn kind(self: &MediaStreamTrack) -> String;
        fn id(self: &MediaStreamTrack) -> String;
        fn enabled(self: &MediaStreamTrack) -> bool;
        fn set_enabled(self: &MediaStreamTrack, enable: bool) -> bool;
        fn state(self: &MediaStreamTrack) -> TrackState;

        fn _shared_media_stream_track() -> SharedPtr<MediaStreamTrack>;
    }
}

impl_thread_safety!(ffi::MediaStreamTrack, Send + Sync);
