#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum VideoFrameBufferType {
        Native,
        I420,
        I420A,
        I422,
        I444,
        I010,
        NV12,
    }

    unsafe extern "C++" {
        include!("livekit/video_frame_buffer.h");

        type VideoFrameBuffer;
        type PlanarYuvBuffer;
        type PlanarYuv8Buffer;
        type I420Buffer;

        fn buffer_type(self: &VideoFrameBuffer) -> VideoFrameBufferType;
        fn width(self: &VideoFrameBuffer) -> i32;
        fn height(self: &VideoFrameBuffer) -> i32;
        fn to_i420(self: Pin<&mut VideoFrameBuffer>) -> SharedPtr<I420Buffer>;

        fn chroma_width(self: &PlanarYuvBuffer) -> i32;
        fn chroma_height(self: &PlanarYuvBuffer) -> i32;
        fn stride_y(self: &PlanarYuvBuffer) -> i32;
        fn stride_u(self: &PlanarYuvBuffer) -> i32;
        fn stride_v(self: &PlanarYuvBuffer) -> i32;

        fn data_y(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_u(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_v(self: &PlanarYuv8Buffer) -> *const u8;

        fn to_video_frame_buffer(buffer: SharedPtr<PlanarYuvBuffer>)
            -> SharedPtr<VideoFrameBuffer>;
        fn to_yuv_buffer(buffer: SharedPtr<PlanarYuv8Buffer>) -> SharedPtr<PlanarYuvBuffer>;
        fn to_yuv8_buffer(buffer: SharedPtr<I420Buffer>) -> SharedPtr<PlanarYuv8Buffer>;
    }
}
