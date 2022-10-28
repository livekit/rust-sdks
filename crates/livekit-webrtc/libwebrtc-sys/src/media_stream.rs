#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
    #[repr(i32)]
    pub enum TrackState {
        Live,
        Ended,
    }

    unsafe extern "C++" {
        include!("livekit/media_stream.h");

        type MediaStreamTrack;
        type MediaStream;

        fn kind(self: &MediaStreamTrack) -> String;
        fn id(self: &MediaStreamTrack) -> String;
        fn enabled(self: &MediaStreamTrack) -> bool;
        fn set_enabled(self: Pin<&mut MediaStreamTrack>, enable: bool) -> bool;
        fn state(self: &MediaStreamTrack) -> TrackState;

        fn id(self: &MediaStream) -> String;

        fn _unique_media_stream_track() -> UniquePtr<MediaStreamTrack>; // Ignore
        fn _unique_media_stream() -> UniquePtr<MediaStream>; // Ignore
    }
}

unsafe impl Sync for ffi::MediaStreamTrack {}

unsafe impl Send for ffi::MediaStreamTrack {}

unsafe impl Sync for ffi::MediaStream {}

unsafe impl Send for ffi::MediaStream {}