use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
    }

    unsafe extern "C++" {
        include!("livekit/encoded_video_frame.h");

        type EncodedVideoFrame;

        fn is_key_frame(self: &EncodedVideoFrame) -> bool;

        fn width(self: &EncodedVideoFrame) -> u16;
        fn height(self: &EncodedVideoFrame) -> u16;
        fn timestamp(self: &EncodedVideoFrame) -> u32;

        fn frame_tracking_id(self: &EncodedVideoFrame) -> SharedPtr<u64>;

        fn payload_type(self: &EncodedVideoFrame) -> u8;
        fn payload_data(self: &EncodedVideoFrame) -> *const u8;
        fn payload_size(self: &EncodedVideoFrame) -> usize;

        fn absolute_capture_timestamp(self: &EncodedVideoFrame) -> SharedPtr<u64>;
        fn estimated_capture_clock_offset(self: &EncodedVideoFrame) -> SharedPtr<i64>;
    }

    impl UniquePtr<EncodedVideoFrame> {}
}

impl_thread_safety!(ffi::EncodedVideoFrame, Send + Sync);