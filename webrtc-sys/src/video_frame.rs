use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum VideoRotation {
        VideoRotation0 = 0,
        VideoRotation90 = 90,
        VideoRotation180 = 180,
        VideoRotation270 = 270,
    }

    unsafe extern "C++" {
        include!("livekit/video_frame.h");
        include!("livekit/video_frame_buffer.h");

        type VideoFrame;
        type VideoFrameBuilder;
        type VideoFrameBuffer = crate::video_frame_buffer::ffi::VideoFrameBuffer;

        fn width(self: &VideoFrame) -> i32;
        fn height(self: &VideoFrame) -> i32;
        fn size(self: &VideoFrame) -> u32;
        fn id(self: &VideoFrame) -> u16;
        fn timestamp_us(self: &VideoFrame) -> i64;
        fn ntp_time_ms(self: &VideoFrame) -> i64;
        fn transport_frame_id(self: &VideoFrame) -> u32;
        fn timestamp(self: &VideoFrame) -> u32;
        fn rotation(self: &VideoFrame) -> VideoRotation;
        fn video_frame_buffer(self: &VideoFrame) -> UniquePtr<VideoFrameBuffer>;

        fn set_video_frame_builder(buffer: UniquePtr<VideoFrameBuffer>);
        fn set_timestamp_us(timestamp_us: i64);
        fn set_rotation(rotation: VideoRotation);
        fn set_id(id: u16);
        fn build() -> UniquePtr<VideoFrame>;

        fn create_video_frame_builder() -> UniquePtr<VideoFrameBuilder>;
    }
}

impl_thread_safety!(ffi::VideoFrame, Send + Sync);
