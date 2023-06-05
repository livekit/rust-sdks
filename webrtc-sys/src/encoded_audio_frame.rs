use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/encoded_audio_frame.h");

        type EncodedAudioFrame;

        fn timestamp(self: &EncodedAudioFrame) -> u32;

        fn payload_type(self: &EncodedAudioFrame) -> u8;
        fn payload_data(self: &EncodedAudioFrame) -> *const u8;
        fn payload_size(self: &EncodedAudioFrame) -> usize;
    }

    impl UniquePtr<EncodedAudioFrame> {}
}

impl_thread_safety!(ffi::EncodedAudioFrame, Send + Sync);