#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/yuv_helper.h");

        unsafe fn i420_to_abgr(
            src_y: *const u8,
            src_stride_y: i32,
            src_u: *const u8,
            src_stride_u: i32,
            src_v: *const u8,
            src_stride_v: i32,
            dst_abgr: *mut u8,
            dst_stride_abgr: i32,
            width: i32,
            height: i32,
        );
    }
}
