use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/encoded_audio_frame.h");

        type EncodedAudioFrame;

        fn timestamp(self: &EncodedAudioFrame) -> u32;

        fn sequence_number(self: &EncodedAudioFrame) -> u16;

        fn payload_type(self: &EncodedAudioFrame) -> u8;
        fn payload_data(self: &EncodedAudioFrame) -> *const u8;
        fn payload_size(self: &EncodedAudioFrame) -> usize;

        fn absolute_capture_timestamp(self: &EncodedAudioFrame) -> SharedPtr<u64>;
        fn estimated_capture_clock_offset(self: &EncodedAudioFrame) -> SharedPtr<i64>;
    }

    impl UniquePtr<EncodedAudioFrame> {}
}

impl_thread_safety!(ffi::EncodedAudioFrame, Send + Sync);