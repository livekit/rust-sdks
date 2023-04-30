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

    extern "C++" {
        include!("livekit/video_frame_buffer.h");

        type VideoFrameBuffer = crate::video_frame_buffer::ffi::VideoFrameBuffer;
    }

    unsafe extern "C++" {
        include!("livekit/video_frame.h");

        type VideoFrame;

        fn width(self: &VideoFrame) -> u32;
        fn height(self: &VideoFrame) -> u32;
        fn size(self: &VideoFrame) -> u32;
        fn id(self: &VideoFrame) -> u16;
        fn timestamp_us(self: &VideoFrame) -> i64;
        fn ntp_time_ms(self: &VideoFrame) -> i64;
        fn transport_frame_id(self: &VideoFrame) -> u32;
        fn timestamp(self: &VideoFrame) -> u32;
        fn rotation(self: &VideoFrame) -> VideoRotation;
        unsafe fn video_frame_buffer(self: &VideoFrame) -> UniquePtr<VideoFrameBuffer>;

        // VideoFrameBuilder
        type VideoFrameBuilder;
        fn new_video_frame_builder() -> UniquePtr<VideoFrameBuilder>;
        fn set_timestamp_us(self: Pin<&mut VideoFrameBuilder>, timestamp_us: i64);
        fn set_rotation(self: Pin<&mut VideoFrameBuilder>, rotation: VideoRotation);
        fn set_id(self: Pin<&mut VideoFrameBuilder>, id: u16);
        fn set_video_frame_buffer(self: Pin<&mut VideoFrameBuilder>, buffer: &VideoFrameBuffer);

        fn build(self: Pin<&mut VideoFrameBuilder>) -> UniquePtr<VideoFrame>;

    }
}

impl_thread_safety!(ffi::VideoFrame, Send + Sync);
