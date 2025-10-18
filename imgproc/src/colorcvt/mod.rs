// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod assert;

macro_rules! x420_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_y: &[u8],
            stride_y: u32,
            src_u: &[u8],
            stride_u: u32,
            src_v: &[u8],
            stride_v: u32,
            dst_rgba: &mut [u8],
            dst_stride_rgba: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_420(src_y, stride_y, src_u, stride_u, src_v, stride_v, width, height);
            assert::valid_rgba(dst_rgba, dst_stride_rgba, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_y.as_ptr(),
                    stride_y as i32,
                    src_u.as_ptr(),
                    stride_u as i32,
                    src_v.as_ptr(),
                    stride_v as i32,
                    dst_rgba.as_mut_ptr(),
                    dst_stride_rgba as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

x420_to_rgba!(i420_to_rgba, rs_I420ToRGBA);
x420_to_rgba!(i420_to_abgr, rs_I420ToABGR);
x420_to_rgba!(i420_to_bgra, rs_I420ToBGRA);
x420_to_rgba!(i420_to_argb, rs_I420ToARGB);
x420_to_rgba!(j420_to_argb, rs_J420ToARGB);
x420_to_rgba!(j420_to_abgr, rs_J420ToABGR);
x420_to_rgba!(h420_to_argb, rs_H420ToARGB);
x420_to_rgba!(h420_to_abgr, rs_H420ToABGR);
x420_to_rgba!(u420_to_argb, rs_U420ToARGB);
x420_to_rgba!(u420_to_abgr, rs_U420ToABGR);

pub fn i420_to_rgb24(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_rgb24: &mut [u8],
    dst_stride_rgb24: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_420(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_rgb24, dst_stride_rgb24, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I420ToRGB24(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_rgb24.as_mut_ptr(),
            dst_stride_rgb24 as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i420_to_raw(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_raw: &mut [u8],
    dst_stride_raw: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_420(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_raw, dst_stride_raw, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I420ToRAW(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_raw.as_mut_ptr(),
            dst_stride_raw as i32,
            width as i32,
            height,
        ) == 0
    });
}

macro_rules! rgba_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_abgr: &[u8],
            src_stride_abgr: u32,
            dst_argb: &mut [u8],
            dst_stride_argb: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_rgba(src_abgr, src_stride_abgr, width, height);
            assert::valid_rgba(dst_argb, dst_stride_argb, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_abgr.as_ptr(),
                    src_stride_abgr as i32,
                    dst_argb.as_mut_ptr(),
                    dst_stride_argb as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

rgba_to_rgba!(abgr_to_argb, rs_ABGRToARGB);
rgba_to_rgba!(argb_to_abgr, rs_ARGBToABGR);
rgba_to_rgba!(rgba_to_argb, rs_RGBAToARGB);
rgba_to_rgba!(bgra_to_argb, rs_BGRAToARGB);

macro_rules! rgba_to_420 {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_rgba: &[u8],
            src_stride_rgba: u32,
            dst_y: &mut [u8],
            dst_stride_y: u32,
            dst_u: &mut [u8],
            dst_stride_u: u32,
            dst_v: &mut [u8],
            dst_stride_v: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_rgba(src_rgba, src_stride_rgba, width, height);
            assert::valid_420(
                dst_y,
                dst_stride_y,
                dst_u,
                dst_stride_u,
                dst_v,
                dst_stride_v,
                width,
                height,
            );

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_rgba.as_ptr(),
                    src_stride_rgba as i32,
                    dst_y.as_mut_ptr(),
                    dst_stride_y as i32,
                    dst_u.as_mut_ptr(),
                    dst_stride_u as i32,
                    dst_v.as_mut_ptr(),
                    dst_stride_v as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

rgba_to_420!(rgba_to_i420, rs_RGBAToI420);
rgba_to_420!(bgra_to_i420, rs_BGRAToI420);
rgba_to_420!(argb_to_i420, rs_ARGBToI420);
rgba_to_420!(abgr_to_i420, rs_ABGRToI420);

pub fn raw_to_i420(
    src_raw: &[u8],
    src_stride_raw: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_rgb(src_raw, src_stride_raw, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    unsafe {
        yuv_sys::rs_RAWToI420(
            src_raw.as_ptr(),
            src_stride_raw as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        )
    };
}

pub fn i422_to_i420(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_422(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I422ToI420(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i444_to_i420(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_444(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I444ToI420(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i010_to_i420(
    src_y: &[u16],
    src_stride_y: u32,
    src_u: &[u16],
    src_stride_u: u32,
    src_v: &[u16],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_010(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I010ToI420(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn nv12_to_i420(
    src_y: &[u8],
    src_stride_y: u32,
    src_uv: &[u8],
    src_stride_uv: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_nv12(src_y, src_stride_y, src_uv, src_stride_uv, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_NV12ToI420(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_uv.as_ptr(),
            src_stride_uv as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i422_to_raw(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_raw: &mut [u8],
    dst_stride_raw: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_422(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_raw, dst_stride_raw, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I422ToRAW(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_raw.as_mut_ptr(),
            dst_stride_raw as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i422_to_rgb24(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_rgb24: &mut [u8],
    dst_stride_rgb24: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_422(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_rgb24, dst_stride_rgb24, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I422ToRGB24(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_rgb24.as_mut_ptr(),
            dst_stride_rgb24 as i32,
            width as i32,
            height,
        ) == 0
    });
}

macro_rules! x422_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_y: &[u8],
            src_stride_y: u32,
            src_u: &[u8],
            src_stride_u: u32,
            src_v: &[u8],
            src_stride_v: u32,
            dst_rgba: &mut [u8],
            dst_stride_rgba: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_422(
                src_y,
                src_stride_y,
                src_u,
                src_stride_u,
                src_v,
                src_stride_v,
                width,
                height,
            );
            assert::valid_rgba(dst_rgba, dst_stride_rgba, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_y.as_ptr(),
                    src_stride_y as i32,
                    src_u.as_ptr(),
                    src_stride_u as i32,
                    src_v.as_ptr(),
                    src_stride_v as i32,
                    dst_rgba.as_mut_ptr(),
                    dst_stride_rgba as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

x422_to_rgba!(i422_to_abgr, rs_I422ToABGR);
x422_to_rgba!(j422_to_argb, rs_J422ToARGB);
x422_to_rgba!(i422_to_bgra, rs_I422ToBGRA);
x422_to_rgba!(i422_to_rgba, rs_I422ToRGBA);

pub fn i444_to_raw(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_raw: &mut [u8],
    dst_stride_raw: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_444(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_raw, dst_stride_raw, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I444ToRAW(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_raw.as_mut_ptr(),
            dst_stride_raw as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i444_to_rgb24(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_rgb24: &mut [u8],
    dst_stride_rgb24: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_444(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_rgb(dst_rgb24, dst_stride_rgb24, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I444ToRGB24(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_rgb24.as_mut_ptr(),
            dst_stride_rgb24 as i32,
            width as i32,
            height,
        ) == 0
    });
}

macro_rules! x444_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_y: &[u8],
            src_stride_y: u32,
            src_u: &[u8],
            src_stride_u: u32,
            src_v: &[u8],
            src_stride_v: u32,
            dst_rgba: &mut [u8],
            dst_stride_rgba: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_444(
                src_y,
                src_stride_y,
                src_u,
                src_stride_u,
                src_v,
                src_stride_v,
                width,
                height,
            );
            assert::valid_rgba(dst_rgba, dst_stride_rgba, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_y.as_ptr(),
                    src_stride_y as i32,
                    src_u.as_ptr(),
                    src_stride_u as i32,
                    src_v.as_ptr(),
                    src_stride_v as i32,
                    dst_rgba.as_mut_ptr(),
                    dst_stride_rgba as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

x444_to_rgba!(i444_to_abgr, rs_I444ToABGR);
x444_to_rgba!(i444_to_argb, rs_I444ToARGB);

macro_rules! x010_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_y: &[u16],
            src_stride_y: u32,
            src_u: &[u16],
            src_stride_u: u32,
            src_v: &[u16],
            src_stride_v: u32,
            dst_abgr: &mut [u8],
            dst_stride_abgr: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_010(
                src_y,
                src_stride_y,
                src_u,
                src_stride_u,
                src_v,
                src_stride_v,
                width,
                height,
            );
            assert::valid_rgba(dst_abgr, dst_stride_abgr, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_y.as_ptr(),
                    src_stride_y as i32,
                    src_u.as_ptr(),
                    src_stride_u as i32,
                    src_v.as_ptr(),
                    src_stride_v as i32,
                    dst_abgr.as_mut_ptr(),
                    dst_stride_abgr as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

x010_to_rgba!(i010_to_abgr, rs_I010ToABGR);
x010_to_rgba!(i010_to_argb, rs_I010ToARGB);

pub fn nv12_to_raw(
    src_y: &[u8],
    src_stride_y: u32,
    src_uv: &[u8],
    src_stride_uv: u32,
    dst_raw: &mut [u8],
    dst_stride_raw: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_nv12(src_y, src_stride_y, src_uv, src_stride_uv, width, height);
    assert::valid_rgb(dst_raw, dst_stride_raw, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_NV12ToRAW(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_uv.as_ptr(),
            src_stride_uv as i32,
            dst_raw.as_mut_ptr(),
            dst_stride_raw as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn nv12_to_rgb24(
    src_y: &[u8],
    src_stride_y: u32,
    src_uv: &[u8],
    src_stride_uv: u32,
    dst_rgb24: &mut [u8],
    dst_stride_rgb24: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_nv12(src_y, src_stride_y, src_uv, src_stride_uv, width, height);
    assert::valid_rgb(dst_rgb24, dst_stride_rgb24, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_NV12ToRGB24(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_uv.as_ptr(),
            src_stride_uv as i32,
            dst_rgb24.as_mut_ptr(),
            dst_stride_rgb24 as i32,
            width as i32,
            height,
        ) == 0
    });
}

macro_rules! nv12_to_rgba {
    ($rust_fnc:ident, $yuv_sys_fnc:ident) => {
        pub fn $rust_fnc(
            src_y: &[u8],
            src_stride_y: u32,
            src_uv: &[u8],
            src_stride_uv: u32,
            dst_rgba: &mut [u8],
            dst_stride_rgba: u32,
            width: u32,
            height: u32,
            flip_y: bool,
        ) {
            assert::valid_nv12(src_y, src_stride_y, src_uv, src_stride_uv, width, height);
            assert::valid_rgba(dst_rgba, dst_stride_rgba, width, height);

            let height = height as i32 * if flip_y { -1 } else { 1 };

            assert!(unsafe {
                yuv_sys::$yuv_sys_fnc(
                    src_y.as_ptr(),
                    src_stride_y as i32,
                    src_uv.as_ptr(),
                    src_stride_uv as i32,
                    dst_rgba.as_mut_ptr(),
                    dst_stride_rgba as i32,
                    width as i32,
                    height,
                ) == 0
            });
        }
    };
}

nv12_to_rgba!(nv12_to_abgr, rs_NV12ToABGR);
nv12_to_rgba!(nv12_to_argb, rs_NV12ToARGB);

pub fn i420_copy(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_420(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_420(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I420Copy(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i420a_copy(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    src_a: &[u8],
    src_stride_a: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    dst_a: &mut [u8],
    dst_stride_a: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_420a(
        src_y,
        src_stride_y,
        src_u,
        src_stride_u,
        src_v,
        src_stride_v,
        src_a,
        src_stride_a,
        width,
        height,
    );

    assert::valid_420a(
        dst_y,
        dst_stride_y,
        dst_u,
        dst_stride_u,
        dst_v,
        dst_stride_v,
        dst_a,
        dst_stride_a,
        width,
        height,
    );

    i420_copy(
        src_y,
        src_stride_y,
        src_u,
        src_stride_u,
        src_v,
        src_stride_v,
        dst_y,
        dst_stride_y,
        dst_u,
        dst_stride_u,
        dst_v,
        dst_stride_v,
        width,
        height,
        flip_y,
    );

    let height = height as i32 * if flip_y { -1 } else { 1 };

    unsafe {
        yuv_sys::rs_CopyPlane(
            src_a.as_ptr(),
            src_stride_a as i32,
            dst_a.as_mut_ptr(),
            dst_stride_a as i32,
            width as i32,
            height,
        )
    }
}

pub fn i422_copy(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_422(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_422(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I422Copy(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i444_copy(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_444(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_444(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I444Copy(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn i010_copy(
    src_y: &[u16],
    src_stride_y: u32,
    src_u: &[u16],
    src_stride_u: u32,
    src_v: &[u16],
    src_stride_v: u32,
    dst_y: &mut [u16],
    dst_stride_y: u32,
    dst_u: &mut [u16],
    dst_stride_u: u32,
    dst_v: &mut [u16],
    dst_stride_v: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_010(src_y, src_stride_y, src_u, src_stride_u, src_v, src_stride_v, width, height);
    assert::valid_010(dst_y, dst_stride_y, dst_u, dst_stride_u, dst_v, dst_stride_v, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(unsafe {
        yuv_sys::rs_I010Copy(
            src_y.as_ptr(),
            src_stride_y as i32,
            src_u.as_ptr(),
            src_stride_u as i32,
            src_v.as_ptr(),
            src_stride_v as i32,
            dst_y.as_mut_ptr(),
            dst_stride_y as i32,
            dst_u.as_mut_ptr(),
            dst_stride_u as i32,
            dst_v.as_mut_ptr(),
            dst_stride_v as i32,
            width as i32,
            height,
        ) == 0
    });
}

pub fn nv12_copy(
    src_y: &[u8],
    src_stride_y: u32,
    src_uv: &[u8],
    src_stride_uv: u32,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_uv: &mut [u8],
    dst_stride_uv: u32,
    width: u32,
    height: u32,
    flip_y: bool,
) {
    assert::valid_nv12(src_y, src_stride_y, src_uv, src_stride_uv, width, height);
    assert::valid_nv12(dst_y, dst_stride_y, dst_uv, dst_stride_uv, width, height);

    let height = height as i32 * if flip_y { -1 } else { 1 };

    assert!(
        unsafe {
            yuv_sys::rs_NV12Copy(
                src_y.as_ptr(),
                src_stride_y as i32,
                src_uv.as_ptr(),
                src_stride_uv as i32,
                dst_y.as_mut_ptr(),
                dst_stride_y as i32,
                dst_uv.as_mut_ptr(),
                dst_stride_uv as i32,
                width as i32,
                height,
            )
        } == 0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data() {
        const WIDTH: usize = 160;
        const HEIGHT: usize = 90;

        let dst_abgr = &mut [0u8; WIDTH * HEIGHT * 4];
        let src_y = &[0u8; WIDTH * HEIGHT];
        let src_u = &[0u8; WIDTH * HEIGHT + 1 / 2];
        let src_v = &[0u8; WIDTH * HEIGHT + 1 / 2];

        i420_to_abgr(
            src_y,
            WIDTH as u32,
            src_u,
            WIDTH as u32 + 1 / 2,
            src_v,
            WIDTH as u32 + 1 / 2,
            dst_abgr,
            WIDTH as u32 * 4,
            WIDTH as u32,
            HEIGHT as u32,
            false,
        );
    }
}
