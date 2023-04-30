use thiserror::Error;
use webrtc_sys::yuv_helper as yuv_sys;

#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("conversion failed: {0}")]
    Convert(&'static str),
}

#[inline]
fn argb_assert_safety(
    src: &[u8],
    src_stride: u32,
    _width: i32,
    height: i32,
) -> Result<(), ConvertError> {
    let height_abs = height.abs() as u32;
    let min = (src_stride * height_abs) as usize;

    if src.len() < min {
        return Err(ConvertError::Convert("dst isn't large enough"));
    }

    Ok(())
}

#[inline]
fn i420_assert_safety(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    _width: i32,
    height: i32,
) -> Result<(), ConvertError> {
    let height_abs = height.abs() as u32;
    let chroma_height = (height_abs + 1) / 2;
    let min_y = (src_stride_y * height_abs) as usize;
    let min_u = (src_stride_u * chroma_height) as usize;
    let min_v = (src_stride_v * chroma_height) as usize;

    if src_y.len() < min_y {
        return Err(ConvertError::Convert("src_y isn't large enough"));
    }

    if src_u.len() < min_u {
        return Err(ConvertError::Convert("src_u isn't large enough"));
    }

    if src_v.len() < min_v {
        return Err(ConvertError::Convert("src_v isn't large enough"));
    }

    Ok(())
}

macro_rules! i420_to_x {
    ($x:ident) => {
        pub fn $x(
            src_y: &[u8],
            src_stride_y: u32,
            src_u: &[u8],
            src_stride_u: u32,
            src_v: &[u8],
            src_stride_v: u32,
            dst: &mut [u8],
            dst_stride: u32,
            width: i32,
            height: i32,
        ) -> Result<(), ConvertError> {
            argb_assert_safety(dst, dst_stride, width, height)?;
            i420_assert_safety(
                src_y,
                src_stride_y,
                src_u,
                src_stride_u,
                src_v,
                src_stride_v,
                width,
                height,
            )?;

            unsafe {
                yuv_sys::ffi::$x(
                    src_y.as_ptr(),
                    src_stride_y as i32,
                    src_u.as_ptr(),
                    src_stride_u as i32,
                    src_v.as_ptr(),
                    src_stride_v as i32,
                    dst.as_mut_ptr(),
                    dst_stride as i32,
                    width,
                    height,
                )
                .unwrap();
            }

            Ok(())
        }
    };
}

macro_rules! x_to_i420 {
    ($x:ident) => {
        pub fn $x(
            src_argb: &[u8],
            src_stride_argb: u32,
            dst_y: &mut [u8],
            dst_stride_y: u32,
            dst_u: &mut [u8],
            dst_stride_u: u32,
            dst_v: &mut [u8],
            dst_stride_v: u32,
            width: i32,
            height: i32,
        ) -> Result<(), ConvertError> {
            argb_assert_safety(src_argb, src_stride_argb, width, height)?;
            i420_assert_safety(
                dst_y,
                dst_stride_y,
                dst_u,
                dst_stride_u,
                dst_v,
                dst_stride_v,
                width,
                height,
            )?;

            unsafe {
                yuv_sys::ffi::$x(
                    src_argb.as_ptr(),
                    src_stride_argb as i32,
                    dst_y.as_mut_ptr(),
                    dst_stride_y as i32,
                    dst_u.as_mut_ptr(),
                    dst_stride_u as i32,
                    dst_v.as_mut_ptr(),
                    dst_stride_v as i32,
                    width,
                    height,
                )
                .unwrap();
            }

            Ok(())
        }
    };
}

pub fn argb_to_rgb24(
    src_argb: &[u8],
    src_stride_argb: u32,
    dst_rgb24: &mut [u8],
    dst_stride_rgb24: u32,
    width: i32,
    height: i32,
) -> Result<(), ConvertError> {
    argb_assert_safety(src_argb, src_stride_argb, width, height)?;
    argb_assert_safety(dst_rgb24, dst_stride_rgb24, width, height)?;

    unsafe {
        yuv_sys::ffi::argb_to_rgb24(
            src_argb.as_ptr(),
            src_stride_argb as i32,
            dst_rgb24.as_mut_ptr(),
            dst_stride_rgb24 as i32,
            width,
            height,
        )
        .unwrap();
    }

    Ok(())
}

x_to_i420!(argb_to_i420);
x_to_i420!(abgr_to_i420);

i420_to_x!(i420_to_argb);
i420_to_x!(i420_to_bgra);
i420_to_x!(i420_to_abgr);
i420_to_x!(i420_to_rgba);
