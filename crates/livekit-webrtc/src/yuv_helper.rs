use std::convert::TryInto;

use libwebrtc_sys::yuv_helper as yuv_sys;

pub fn i420_to_abgr(
    src_y: &[u8],
    src_stride_y: i32,
    src_u: &[u8],
    src_stride_u: i32,
    src_v: &[u8],
    src_stride_v: i32,
    dst_abgr: &mut [u8],
    dst_stride_abgr: i32,
    width: i32,
    height: i32,
) {
    // Assert minimum capacity for safety
    let chroma_height = (height + 1) / 2; // the buffer should be padded?
    let min_y: usize = (src_stride_y * height).try_into().unwrap();
    let min_u: usize = (src_stride_u * chroma_height).try_into().unwrap();
    let min_v: usize = (src_stride_v * chroma_height).try_into().unwrap();
    let min_abgr: usize = (dst_stride_abgr * height).try_into().unwrap();

    assert!(src_y.len() >= min_y);
    assert!(src_u.len() >= min_u);
    assert!(src_v.len() >= min_v);
    assert!(dst_abgr.len() >= min_abgr);

    unsafe {
        yuv_sys::ffi::i420_to_abgr(
            src_y.as_ptr(),
            src_stride_y,
            src_u.as_ptr(),
            src_stride_u,
            src_v.as_ptr(),
            src_stride_v,
            dst_abgr.as_mut_ptr(),
            dst_stride_abgr,
            width,
            height,
        );
    }
}
