#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/media_stream_interface.h");

        type MediaStreamInterface;

        fn _unique_media_stream() -> UniquePtr<MediaStreamInterface>; // Ignore
    }
}
