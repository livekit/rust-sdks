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

        // Require ownership
        unsafe fn to_i420(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420Buffer>;
        unsafe fn get_i420(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420Buffer>;
        // TODO(theomonnom): Bridge other get_*

        fn chroma_width(self: &PlanarYuvBuffer) -> i32;
        fn chroma_height(self: &PlanarYuvBuffer) -> i32;
        fn stride_y(self: &PlanarYuvBuffer) -> i32;
        fn stride_u(self: &PlanarYuvBuffer) -> i32;
        fn stride_v(self: &PlanarYuvBuffer) -> i32;

        fn data_y(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_u(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_v(self: &PlanarYuv8Buffer) -> *const u8;

        unsafe fn yuv_to_vfb(yuv: *const PlanarYuvBuffer) -> *const VideoFrameBuffer;
        unsafe fn yuv8_to_yuv(yuv8: *const PlanarYuv8Buffer) -> *const PlanarYuvBuffer;
        unsafe fn i420_to_yuv8(i420: *const I420Buffer) -> *const PlanarYuv8Buffer;

        fn _unique_video_frame_buffer() -> UniquePtr<VideoFrameBuffer>;
    }
}
