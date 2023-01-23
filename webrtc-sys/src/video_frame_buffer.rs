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
        type PlanarYuv16BBuffer;
        type BiplanarYuvBuffer;
        type BiplanarYuv8Buffer;
        type I420Buffer;
        type I420ABuffer;
        type I422Buffer;
        type I444Buffer;
        type I010Buffer;
        type NV12Buffer;

        fn buffer_type(self: &VideoFrameBuffer) -> VideoFrameBufferType;
        fn width(self: &VideoFrameBuffer) -> i32;
        fn height(self: &VideoFrameBuffer) -> i32;

        /// # SAFETY
        /// The functions require ownership
        unsafe fn to_i420(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420Buffer>;
        unsafe fn get_i420(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420Buffer>;
        unsafe fn get_i420a(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I420ABuffer>;
        unsafe fn get_i422(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I422Buffer>;
        unsafe fn get_i444(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I444Buffer>;
        unsafe fn get_i010(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<I010Buffer>;
        unsafe fn get_nv12(self: Pin<&mut VideoFrameBuffer>) -> UniquePtr<NV12Buffer>;

        fn chroma_width(self: &PlanarYuvBuffer) -> i32;
        fn chroma_height(self: &PlanarYuvBuffer) -> i32;
        fn stride_y(self: &PlanarYuvBuffer) -> i32;
        fn stride_u(self: &PlanarYuvBuffer) -> i32;
        fn stride_v(self: &PlanarYuvBuffer) -> i32;

        fn data_y(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_u(self: &PlanarYuv8Buffer) -> *const u8;
        fn data_v(self: &PlanarYuv8Buffer) -> *const u8;

        fn data_y(self: &PlanarYuv16BBuffer) -> *const u16;
        fn data_u(self: &PlanarYuv16BBuffer) -> *const u16;
        fn data_v(self: &PlanarYuv16BBuffer) -> *const u16;

        fn chroma_width(self: &BiplanarYuvBuffer) -> i32;
        fn chroma_height(self: &BiplanarYuvBuffer) -> i32;
        fn stride_y(self: &BiplanarYuvBuffer) -> i32;
        fn stride_uv(self: &BiplanarYuvBuffer) -> i32;

        fn data_y(self: &BiplanarYuv8Buffer) -> *const u8;
        fn data_uv(self: &BiplanarYuv8Buffer) -> *const u8;

        unsafe fn yuv_to_vfb(yuv: *const PlanarYuvBuffer) -> *const VideoFrameBuffer;
        unsafe fn biyuv_to_vfb(yuv: *const BiplanarYuvBuffer) -> *const VideoFrameBuffer;
        unsafe fn yuv8_to_yuv(yuv8: *const PlanarYuv8Buffer) -> *const PlanarYuvBuffer;
        unsafe fn yuv16b_to_yuv(yuv16b: *const PlanarYuv16BBuffer) -> *const PlanarYuvBuffer;
        unsafe fn biyuv8_to_biyuv(biyuv8: *const BiplanarYuv8Buffer) -> *const BiplanarYuvBuffer;
        unsafe fn i420_to_yuv8(i420: *const I420Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i420a_to_yuv8(i420a: *const I420ABuffer) -> *const PlanarYuv8Buffer;
        unsafe fn i422_to_yuv8(i422: *const I422Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i444_to_yuv8(i444: *const I444Buffer) -> *const PlanarYuv8Buffer;
        unsafe fn i010_to_yuv16b(i010: *const I010Buffer) -> *const PlanarYuv16BBuffer;
        unsafe fn nv12_to_biyuv8(nv12: *const NV12Buffer) -> *const BiplanarYuv8Buffer;

        fn _unique_video_frame_buffer() -> UniquePtr<VideoFrameBuffer>;
    }
}
