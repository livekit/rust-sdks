use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/encoded_video_frame.h");

        type EncodedVideoFrame;

        fn is_key_frame(self: &EncodedVideoFrame) -> bool;
    }

    impl UniquePtr<EncodedVideoFrame> {}
}

impl_thread_safety!(ffi::EncodedVideoFrame, Send + Sync);