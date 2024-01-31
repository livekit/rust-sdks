use std::slice;

use super::{FfiError, FfiResult, FfiServer};
use crate::proto;

pub fn on_video_convert(
    server: &'static FfiServer,
    video_convert: proto::VideoConvertRequest,
) -> FfiResult<proto::VideoConvertResponse> {
    let Some(buffer) = video_convert.buffer else {
        return Err(FfiError::InvalidRequest("buffer is empty".into()));
    };

    let data =
        unsafe { slice::from_raw_parts(buffer.data_ptr as *const u8, buffer.data_len as usize) };

    let info = match buffer.r#type() {
        proto::VideoBufferType::Rgba => {
            rgba_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::Abgr => {
            abgr_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::Argb => {
            argb_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::Bgra => {
            bgra_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::Rgb24 => {
            rgb24_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::I420 => {
            i420_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::I420a => {
            i420a_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::I422 => {
            i422_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::I444 => {
            i444_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::I010 => {
            i010_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
        proto::VideoBufferType::Nv12 => {
            nv12_to_x(server, buffer, data, video_convert.dst_type(), video_convert.flip_y)
        }
    };

    match info {
        Ok(info) => Ok(proto::VideoConvertResponse { buffer: Some(info), error: None }),
        Err(err) => Ok(proto::VideoConvertResponse { buffer: None, error: Some(err.to_string()) }),
    }
}

fn rgba_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let stride = buffer.stride;

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::abgr_to_i420(
                data,
                stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("rgba to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn abgr_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let stride = buffer.stride;

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::rgba_to_i420(
                data,
                stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("abgr to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn argb_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let stride = buffer.stride;

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::bgra_to_i420(
                data,
                stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("argb to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn bgra_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let stride = buffer.stride;

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::argb_to_i420(
                data,
                stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("bgra to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn rgb24_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Rgba);
    let stride = buffer.stride;

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::raw_to_i420(
                data,
                stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("raw to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn i420_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I420);

    let (cmp0, cmp1, cmp2) = { (buffer.components[0], buffer.components[1], buffer.components[2]) };
    let (src_y, src_u, src_v) =
        split_i420(data, cmp0.stride, cmp1.stride, cmp2.stride, buffer.height);

    match dst_type {
        proto::VideoBufferType::Rgba
        | proto::VideoBufferType::Abgr
        | proto::VideoBufferType::Argb
        | proto::VideoBufferType::Bgra => {
            let mut dst_rgba = vec![0u8; (buffer.width * buffer.height * 4) as usize];
            let dst_stride_rgba = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            src_y,
                            cmp0.stride,
                            src_u,
                            cmp1.stride,
                            src_v,
                            cmp2.stride,
                            dst_rgba.as_mut_slice(),
                            dst_stride_rgba,
                            buffer.width,
                            buffer.height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i420_to_abgr);
            cvt!(proto::VideoBufferType::Abgr, i420_to_rgba);
            cvt!(proto::VideoBufferType::Argb, i420_to_bgra);
            cvt!(proto::VideoBufferType::Bgra, i420_to_argb);

            let id = server.next_id();
            server.store_handle(id, dst_rgba);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgba_info(dst_rgba.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::Rgb24 => {
            let mut dst_raw = vec![0u8; (buffer.width * buffer.height * 3) as usize];
            let dst_stride_raw = buffer.width * 3;

            imgproc::colorcvt::i420_to_raw(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_raw.as_mut_slice(),
                dst_stride_raw,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_raw);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgb_info(dst_raw.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::i420_copy(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i420 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn i420a_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I420a);

    let (cmp0, cmp1, cmp2, cmp3) = {
        (buffer.components[0], buffer.components[1], buffer.components[2], buffer.components[3])
    };
    let (src_y, src_u, src_v, _src_a) =
        split_i420a(data, cmp0.stride, cmp1.stride, cmp2.stride, cmp3.stride, buffer.height);

    match dst_type {
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::i420_copy(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );
            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i420a to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn i422_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I422);

    let (cmp0, cmp1, cmp2) = { (buffer.components[0], buffer.components[1], buffer.components[2]) };
    let (src_y, src_u, src_v) =
        split_i422(data, cmp0.stride, cmp1.stride, cmp2.stride, buffer.height);

    match dst_type {
        proto::VideoBufferType::Rgba
        | proto::VideoBufferType::Abgr
        | proto::VideoBufferType::Argb => {
            let mut dst_rgba = vec![0u8; (buffer.width * buffer.height * 4) as usize];
            let dst_stride_rgba = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            src_y,
                            cmp0.stride,
                            src_u,
                            cmp1.stride,
                            src_v,
                            cmp2.stride,
                            dst_rgba.as_mut_slice(),
                            dst_stride_rgba,
                            buffer.width,
                            buffer.height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i422_to_abgr);
            cvt!(proto::VideoBufferType::Abgr, i422_to_rgba);
            cvt!(proto::VideoBufferType::Argb, i422_to_bgra);

            let id = server.next_id();
            server.store_handle(id, dst_rgba);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgba_info(dst_rgba.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::i422_to_i420(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );
            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i422 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn i444_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I444);

    let (cmp0, cmp1, cmp2) = { (buffer.components[0], buffer.components[1], buffer.components[2]) };
    let (src_y, src_u, src_v) =
        split_i444(data, cmp0.stride, cmp1.stride, cmp2.stride, buffer.height);

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst_rgba = vec![0u8; (buffer.width * buffer.height * 4) as usize];
            let dst_stride_rgba = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            src_y,
                            cmp0.stride,
                            src_u,
                            cmp1.stride,
                            src_v,
                            cmp2.stride,
                            dst_rgba.as_mut_slice(),
                            dst_stride_rgba,
                            buffer.width,
                            buffer.height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i444_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, i444_to_argb);

            let id = server.next_id();
            server.store_handle(id, dst_rgba);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgba_info(dst_rgba.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::i444_to_i420(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i444 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn i010_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::I010);

    let (cmp0, cmp1, cmp2) = { (buffer.components[0], buffer.components[1], buffer.components[2]) };
    let (src_y, src_u, src_v) =
        split_i010(data, cmp0.stride, cmp1.stride, cmp2.stride, buffer.height);

    let (_, src_y, _) = unsafe { src_y.align_to_mut::<u16>() };
    let (_, src_u, _) = unsafe { src_y.align_to_mut::<u16>() };
    let (_, src_v, _) = unsafe { src_y.align_to_mut::<u16>() };

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst_rgba = vec![0u8; (buffer.width * buffer.height * 4) as usize];
            let dst_stride_rgba = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            src_y,
                            cmp0.stride,
                            src_u,
                            cmp1.stride,
                            src_v,
                            cmp2.stride,
                            dst_rgba.as_mut_slice(),
                            dst_stride_rgba,
                            buffer.width,
                            buffer.height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, i010_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, i010_to_argb);

            let id = server.next_id();
            server.store_handle(id, dst_rgba);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgba_info(dst_rgba.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::i010_to_i420(
                src_y,
                cmp0.stride,
                src_u,
                cmp1.stride,
                src_v,
                cmp2.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("i010 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn nv12_to_x(
    server: &'static FfiServer,
    buffer: proto::VideoBufferInfo,
    data: &[u8],
    dst_type: proto::VideoBufferType,
    flip_y: bool,
) -> FfiResult<proto::OwnedVideoBuffer> {
    assert_eq!(buffer.r#type(), proto::VideoBufferType::Nv12);

    let (cmp0, cmp1) = { (buffer.components[0], buffer.components[1]) };
    let (src_y, src_uv) = split_nv12(data, cmp0.stride, cmp1.stride, buffer.height);

    match dst_type {
        proto::VideoBufferType::Rgba | proto::VideoBufferType::Bgra => {
            let mut dst_rgba = vec![0u8; (buffer.width * buffer.height * 4) as usize];
            let dst_stride_rgba = buffer.width * 4;

            macro_rules! cvt {
                ($rgba:expr, $fnc:ident) => {
                    if dst_type == $rgba {
                        imgproc::colorcvt::$fnc(
                            src_y,
                            cmp0.stride,
                            src_uv,
                            cmp1.stride,
                            dst_rgba.as_mut_slice(),
                            dst_stride_rgba,
                            buffer.width,
                            buffer.height,
                            flip_y,
                        );
                    }
                };
            }

            cvt!(proto::VideoBufferType::Rgba, nv12_to_abgr);
            cvt!(proto::VideoBufferType::Bgra, nv12_to_argb);

            let id = server.next_id();
            server.store_handle(id, dst_rgba);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(rgba_info(dst_rgba.as_slice(), dst_type, buffer.width, buffer.height)),
            });
        }
        proto::VideoBufferType::I420 => {
            let chroma_width = (buffer.width + 1) / 2;
            let chroma_height = (buffer.height + 1) / 2;
            let mut dst_i420 = vec![
                0u8;
                (buffer.width * buffer.height + chroma_width * chroma_height * 2)
                    as usize
            ];
            let (dst_y, dst_u, dst_v) = split_i420_mut(
                dst_i420.as_mut_slice(),
                buffer.width,
                chroma_width,
                chroma_width,
                buffer.height,
            );

            imgproc::colorcvt::nv12_to_i420(
                src_y,
                cmp0.stride,
                src_uv,
                cmp1.stride,
                dst_y,
                buffer.width,
                dst_u,
                chroma_width,
                dst_v,
                chroma_width,
                buffer.width,
                buffer.height,
                flip_y,
            );

            let id = server.next_id();
            server.store_handle(id, dst_i420);
            return Ok(proto::OwnedVideoBuffer {
                handle: Some(proto::FfiOwnedHandle { id }),
                info: Some(i420_info(dst_i420.as_slice(), buffer.width, buffer.height)),
            });
        }
        _ => {
            return Err(FfiError::InvalidRequest(
                format!("nv12 to {:?} is not supported", dst_type).into(),
            ))
        }
    }
}

fn split_i420_mut(
    src: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&mut [u8], &mut [u8], &mut [u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * chroma_height) as usize);
    (luma, u, v)
}

fn split_i420(
    dst: &[u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = dst.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * chroma_height) as usize);
    (luma, u, v)
}

fn split_i420a(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    stride_a: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * chroma_height) as usize);
    let (v, a) = v.split_at((stride_v * chroma_height) as usize);
    (luma, u, v, a)
}

fn split_i422(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * height) as usize);
    (luma, u, v)
}

fn split_i444(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * height) as usize);
    (luma, u, v)
}

fn split_i010(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * chroma_height) as usize);
    (luma, u, v)
}

fn split_nv12(src: &[u8], stride_y: u32, stride_uv: u32, height: u32) -> (&[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_uv * chroma_height) as usize);
    (luma, u)
}

fn i420_info(data: &[u8], width: u32, height: u32) -> proto::VideoBufferInfo {
    let chroma_width = (width + 1) / 2;
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(3);
    let c1 =
        proto::video_buffer_info::ComponentInfo { offset: 0, stride: width, size: width * height };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: chroma_width,
        size: chroma_width * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
        stride: chroma_width,
        size: chroma_width * chroma_height,
    };

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I420.into(),
        components,
        data_ptr: data.as_ptr() as u64,
        data_len: data.len() as u32,
        stride: 0,
    }
}

fn rgba_info(
    data: &[u8],
    r#type: proto::VideoBufferType,
    width: u32,
    height: u32,
) -> proto::VideoBufferInfo {
    proto::VideoBufferInfo {
        width,
        height,
        r#type: r#type.into(),
        components: Vec::default(),
        data_ptr: data.as_ptr() as u64,
        data_len: data.len() as u32,
        stride: width * 4,
    }
}

fn rgb_info(
    data: &[u8],
    r#type: proto::VideoBufferType,
    width: u32,
    height: u32,
) -> proto::VideoBufferInfo {
    proto::VideoBufferInfo {
        width,
        height,
        r#type: r#type.into(),
        components: Vec::default(),
        data_ptr: data.as_ptr() as u64,
        data_len: data.len() as u32,
        stride: width * 3,
    }
}
