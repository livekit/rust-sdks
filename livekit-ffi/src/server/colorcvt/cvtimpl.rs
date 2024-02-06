use super::*;
use crate::proto;
use crate::{FfiError, FfiResult};
use imgproc::colorcvt;

pub unsafe fn cvt(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    match buffer.r#type() {
        proto::VideoBufferType::Rgba => cvt_rgba(buffer, dst_type, flip_y),
        proto::VideoBufferType::Abgr => cvt_abgr(buffer, dst_type, flip_y),
        proto::VideoBufferType::Argb => cvt_argb(buffer, dst_type, flip_y),
        proto::VideoBufferType::Bgra => cvt_bgra(buffer, dst_type, flip_y),
        proto::VideoBufferType::Rgb24 => cvt_rgb24(buffer, dst_type, flip_y),
        proto::VideoBufferType::I420 => cvt_i420(buffer, dst_type, flip_y),
        proto::VideoBufferType::I420a => cvt_i420a(buffer, dst_type, flip_y),
        proto::VideoBufferType::I422 => cvt_i422(buffer, dst_type, flip_y),
        proto::VideoBufferType::I444 => cvt_i444(buffer, dst_type, flip_y),
        proto::VideoBufferType::I010 => cvt_i010(buffer, dst_type, flip_y),
        proto::VideoBufferType::Nv12 => cvt_nv12(buffer, dst_type, flip_y),
    }
}

pub unsafe fn cvt_rgba(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let proto::VideoBufferInfo { stride, width, height, data_ptr, .. } = buffer;
    let data_len = (stride * height) as usize;
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();

            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::abgr_to_i420(
                data, stride, dst_y, width, dst_u, chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            Err(FfiError::InvalidRequest(format!("rgba to {:?} is not supported", dst_type).into()))
        }
    }
}

pub unsafe fn cvt_abgr(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let proto::VideoBufferInfo { stride, width, height, data_ptr, .. } = buffer;
    let data_len = (stride * height) as usize;
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();

            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            imgproc::colorcvt::rgba_to_i420(
                data, stride, dst_y, width, dst_u, chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            Err(FfiError::InvalidRequest(format!("abgr to {:?} is not supported", dst_type).into()))
        }
    }
}

pub unsafe fn cvt_argb(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Argb);
    let proto::VideoBufferInfo { stride, width, height, data_ptr, .. } = buffer;
    let data_len = (stride * height) as usize;
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::bgra_to_i420(
                data, stride, dst_y, width, dst_u, chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            Err(FfiError::InvalidRequest(format!("argb to {:?} is not supported", dst_type).into()))
        }
    }
}

pub unsafe fn cvt_bgra(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Bgra);
    let proto::VideoBufferInfo { stride, width, height, data_ptr, .. } = buffer;
    let data_len = (stride * height) as usize;
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::argb_to_i420(
                data, stride, dst_y, width, dst_u, chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            Err(FfiError::InvalidRequest(format!("bgra to {:?} is not supported", dst_type).into()))
        }
    }
}

pub unsafe fn cvt_rgb24(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgb24);
    let proto::VideoBufferInfo { stride, width, height, data_ptr, .. } = buffer;
    let data_len = (stride * height) as usize;
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::raw_to_i420(
                data, stride, dst_y, width, dst_u, chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("rgb24 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_i420(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I420);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
    let (data_y, data_u, data_v) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
            slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
            slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
        )
    };

    match dst_type {
        proto::VideoBufferType::Rgba
        | proto::VideoBufferType::Abgr
        | proto::VideoBufferType::Argb
        | proto::VideoBufferType::Bgra => {
            let mut dst = vec![0u8; (width * height * 4) as usize].into_boxed_slice();
            let stride = width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        colorcvt::$fnc(
                            data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, &mut dst,
                            stride, width, height, flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i420_to_abgr);
            cvt!(proto::VideoBufferType::Abgr, i420_to_rgba);
            cvt!(proto::VideoBufferType::Argb, i420_to_bgra);
            cvt!(proto::VideoBufferType::Bgra, i420_to_argb);

            let info = rgba_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::Rgb24 => {
            let mut dst = vec![0u8; (width * height * 3) as usize].into_boxed_slice();
            let stride = width * 3;

            colorcvt::i420_to_raw(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, &mut dst, stride, width,
                height, flip_y,
            );

            let info = rgb_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i420_copy(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i420 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_i420a(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I420a);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1, c2, c3) = (&components[0], &components[1], &components[2], &components[3]);
    let (data_y, data_u, data_v, data_a) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
            slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
            slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
            slice::from_raw_parts(c3.data_ptr as *const u8, c3.size as usize),
        )
    };

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i420_copy(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );
            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        proto::VideoBufferType::I420a => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2 + width * height) as usize]
                    .into_boxed_slice();

            let (dst_y, dst_u, dst_v, dst_a) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, va) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                let (v, a) = va.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v, a)
            };

            colorcvt::i420a_copy(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, data_a, c3.stride, dst_y,
                width, dst_u, chroma_w, dst_v, chroma_w, dst_a, width, width, height, flip_y,
            );

            let info = i420a_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                dst_a.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
                width,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i420a to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_i422(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I422);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
    let (data_y, data_u, data_v) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
            slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
            slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
        )
    };

    match dst_type {
        proto::VideoBufferType::Rgba
        | proto::VideoBufferType::Abgr
        | proto::VideoBufferType::Argb => {
            let mut dst = vec![0u8; (buffer.width * buffer.height * 4) as usize].into_boxed_slice();
            let stride = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        colorcvt::$fnc(
                            data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, &mut dst,
                            stride, width, height, flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i422_to_abgr);
            cvt!(proto::VideoBufferType::Abgr, i422_to_rgba);
            cvt!(proto::VideoBufferType::Argb, i422_to_bgra);

            let info = rgba_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i422_to_i420(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        proto::VideoBufferType::I422 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = height;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i422_copy(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );
            let info = i422_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i422 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_i444(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I444);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
    let (data_y, data_u, data_v) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
            slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
            slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
        )
    };

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst = vec![0u8; (buffer.width * buffer.height * 4) as usize].into_boxed_slice();
            let stride = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, &mut dst,
                            stride, width, height, flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i444_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, i444_to_argb);

            let info = rgba_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::I420 => {
            let chroma_w = (width + 1) / 2;
            let chroma_h = (height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i444_to_i420(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        proto::VideoBufferType::I444 => {
            let chroma_w = width;
            let chroma_h = height;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();

            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            colorcvt::i444_copy(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i444_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                data_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i444 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_i010(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I010);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
    let (data_y, data_u, data_v) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u16, c0.size as usize / 2),
            slice::from_raw_parts(c1.data_ptr as *const u16, c1.size as usize / 2),
            slice::from_raw_parts(c2.data_ptr as *const u16, c2.size as usize / 2),
        )
    };

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst = vec![0u8; (buffer.width * buffer.height * 4) as usize].into_boxed_slice();
            let stride = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, &mut dst,
                            stride, width, height, flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i010_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, i010_to_argb);

            let info = rgba_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::I420 => {
            let chroma_w = (buffer.width + 1) / 2;
            let chroma_h = (buffer.height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            imgproc::colorcvt::i010_to_i420(
                data_y, c0.stride, data_u, c1.stride, data_v, c2.stride, dst_y, width, dst_u,
                chroma_w, dst_v, chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i010 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

pub unsafe fn cvt_nv12(
    buffer: proto::VideoBufferInfo,
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Nv12);
    let proto::VideoBufferInfo { width, height, components, .. } = buffer;

    let (c0, c1) = (&components[0], &components[1]);
    let (data_y, data_uv) = unsafe {
        (
            slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
            slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
        )
    };

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst = vec![0u8; (buffer.width * buffer.height * 4) as usize].into_boxed_slice();
            let stride = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            data_y, c0.stride, data_uv, c1.stride, &mut dst, stride, width, height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, nv12_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, nv12_to_argb);

            let info = rgba_info(dst.as_ptr(), dst_type, width, height);
            Ok((dst, info))
        }
        proto::VideoBufferType::I420 => {
            let chroma_w = (buffer.width + 1) / 2;
            let chroma_h = (buffer.height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_u, dst_v) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                let (u, v) = chroma.split_at_mut((chroma_w * chroma_h) as usize);
                (luma, u, v)
            };

            imgproc::colorcvt::nv12_to_i420(
                data_y, c0.stride, data_uv, c1.stride, dst_y, width, dst_u, chroma_w, dst_v,
                chroma_w, width, height, flip_y,
            );

            let info = i420_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_u.as_ptr(),
                dst_v.as_ptr(),
                width,
                height,
                width,
                chroma_w,
                chroma_w,
            );
            Ok((dst, info))
        }
        proto::VideoBufferType::Nv12 => {
            let chroma_w = (buffer.width + 1) / 2;
            let chroma_h = (buffer.height + 1) / 2;
            let mut dst =
                vec![0u8; (width * height + chroma_w * chroma_h * 2) as usize].into_boxed_slice();
            let (dst_y, dst_uv) = {
                let (luma, chroma) = dst.split_at_mut((width * height) as usize);
                (luma, chroma)
            };

            imgproc::colorcvt::nv12_copy(
                data_y, c0.stride, data_uv, c1.stride, dst_y, width, dst_uv, chroma_w, width,
                height, flip_y,
            );

            let info = nv12_info(
                dst_y.as_ptr(),
                dst_y.as_ptr(),
                dst_uv.as_ptr(),
                width,
                height,
                width,
                chroma_w,
            );
            Ok((dst, info))
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("nv12 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}
