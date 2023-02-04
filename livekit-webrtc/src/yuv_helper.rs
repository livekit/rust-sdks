use thiserror::Error;

use webrtc_sys::yuv_helper as yuv_sys;

#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("conversion failed: {0}")]
    Convert(&'static str),
}

fn i420_safety(
    src_y: &[u8],
    src_stride_y: i32,
    src_u: &[u8],
    src_stride_u: i32,
    src_v: &[u8],
    src_stride_v: i32,
    dst: &mut [u8],
    dst_stride: i32,
    _width: i32,
    height: i32,
) -> Result<(), ConvertError> {
    let chroma_height = (height + 1) / 2;
    let min_y = (src_stride_y * height) as usize;
    let min_u = (src_stride_u * chroma_height) as usize;
    let min_v = (src_stride_v * chroma_height) as usize;
    let min_dst = (dst_stride * height) as usize;

    if src_y.len() < min_y {
        return Err(ConvertError::Convert("src_y isn't large enough"));
    }

    if src_u.len() < min_u {
        return Err(ConvertError::Convert("src_u isn't large enough"));
    }

    if src_v.len() < min_v {
        return Err(ConvertError::Convert("src_v isn't large enough"));
    }

    if dst.len() < min_dst {
        return Err(ConvertError::Convert("dst isn't large enough"));
    }

    Ok(())
}

macro_rules! i420_to_x {
    ($x:ident) => {
        pub fn $x(
            src_y: &[u8],
            src_stride_y: i32,
            src_u: &[u8],
            src_stride_u: i32,
            src_v: &[u8],
            src_stride_v: i32,
            dst: &mut [u8],
            dst_stride: i32,
            width: i32,
            height: i32,
        ) -> Result<(), ConvertError> {
            i420_safety(
                src_y,
                src_stride_y,
                src_u,
                src_stride_u,
                src_v,
                src_stride_v,
                dst,
                dst_stride,
                width,
                height,
            )?;

            unsafe {
                yuv_sys::ffi::$x(
                    src_y.as_ptr(),
                    src_stride_y,
                    src_u.as_ptr(),
                    src_stride_u,
                    src_v.as_ptr(),
                    src_stride_v,
                    dst.as_mut_ptr(),
                    dst_stride,
                    width,
                    height,
                );
            }

            Ok(())
        }
    };
}

i420_to_x!(i420_to_argb);
i420_to_x!(i420_to_bgra);
i420_to_x!(i420_to_abgr);
i420_to_x!(i420_to_rgba);
