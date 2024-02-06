use crate::{proto, FfiResult};
use livekit::webrtc::{prelude::*, video_frame::BoxVideoBuffer};
use std::slice;

pub mod cvtimpl;

macro_rules! to_i420 {
    ($buffer:ident) => {{
        let proto::VideoBufferInfo { width, height, components, .. } = $buffer;
        let (c0, c1, c2) = (&components[0], &components[1], &components[2]);

        let (data_y, data_u, data_v) = unsafe {
            (
                slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
                slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
                slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
            )
        };

        let mut i420 = I420Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

        let (dy, du, dv) = i420.data_mut();
        dy.copy_from_slice(data_y);
        du.copy_from_slice(data_u);
        dv.copy_from_slice(data_v);
        Box::new(i420) as BoxVideoBuffer
    }};
}

pub unsafe fn to_libwebrtc_buffer(info: proto::VideoBufferInfo) -> BoxVideoBuffer {
    let r#type = info.r#type();
    let proto::VideoBufferInfo { width, height, components, .. } = info.clone();

    match r#type {
        // For rgba buffer, automatically convert to I420
        proto::VideoBufferType::Rgba => {
            let (_data, info) =
                cvtimpl::cvt_rgba(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info)
        }
        proto::VideoBufferType::Abgr => {
            let (_data, info) =
                cvtimpl::cvt_abgr(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info)
        }
        proto::VideoBufferType::Argb => {
            let (_data, info) =
                cvtimpl::cvt_argb(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info)
        }
        proto::VideoBufferType::Bgra => {
            let (_data, info) =
                cvtimpl::cvt_bgra(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info)
        }
        proto::VideoBufferType::Rgb24 => {
            let (_data, info) =
                cvtimpl::cvt_rgb24(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info)
        }
        proto::VideoBufferType::I420 | proto::VideoBufferType::I420a => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);

            let (data_y, data_u, data_v) = unsafe {
                (
                    slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
                    slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
                    slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
                )
            };
            let mut i420 = I420Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i420.data_mut();
            dy.copy_from_slice(data_y);
            du.copy_from_slice(data_u);
            dv.copy_from_slice(data_v);
            Box::new(i420) as BoxVideoBuffer
        }
        proto::VideoBufferType::I422 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);

            let (data_y, data_u, data_v) = unsafe {
                (
                    slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
                    slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
                    slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
                )
            };

            let mut i422 = I422Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i422.data_mut();
            dy.copy_from_slice(data_y);
            du.copy_from_slice(data_u);
            dv.copy_from_slice(data_v);
            Box::new(i422) as BoxVideoBuffer
        }
        proto::VideoBufferType::I444 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);

            let (data_y, data_u, data_v) = unsafe {
                (
                    slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
                    slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
                    slice::from_raw_parts(c2.data_ptr as *const u8, c2.size as usize),
                )
            };
            let mut i444 = I444Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i444.data_mut();
            dy.copy_from_slice(data_y);
            du.copy_from_slice(data_u);
            dv.copy_from_slice(data_v);
            Box::new(i444) as BoxVideoBuffer
        }
        proto::VideoBufferType::I010 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);

            let (data_y, data_u, data_v) = unsafe {
                (
                    slice::from_raw_parts(c0.data_ptr as *const u16, c0.size as usize / 2),
                    slice::from_raw_parts(c1.data_ptr as *const u16, c1.size as usize / 2),
                    slice::from_raw_parts(c2.data_ptr as *const u16, c2.size as usize / 2),
                )
            };

            let mut i010 = I010Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i010.data_mut();
            dy.copy_from_slice(data_y);
            du.copy_from_slice(data_u);
            dv.copy_from_slice(data_v);
            Box::new(i010) as BoxVideoBuffer
        }
        proto::VideoBufferType::Nv12 => {
            let (c0, c1) = (&components[0], &components[1]);

            let (data_y, data_uv) = unsafe {
                (
                    slice::from_raw_parts(c0.data_ptr as *const u8, c0.size as usize),
                    slice::from_raw_parts(c1.data_ptr as *const u8, c1.size as usize),
                )
            };
            let mut nv12 = NV12Buffer::with_strides(info.width, info.height, c0.stride, c1.stride);

            let (dy, duv) = nv12.data_mut();
            dy.copy_from_slice(data_y);
            duv.copy_from_slice(data_uv);
            Box::new(nv12) as BoxVideoBuffer
        }
    }
}

pub fn to_video_buffer_info(
    rtcbuffer: BoxVideoBuffer,
    dst_type: Option<proto::VideoBufferType>,
    _normalize_stride: bool, // always normalize stride for now..
) -> FfiResult<(Box<[u8]>, proto::VideoBufferInfo)> {
    match rtcbuffer.buffer_type() {
        // Convert Native buffer to I420
        VideoBufferType::Native => {
            let i420 = rtcbuffer.to_i420();
            let (width, height) = (i420.width(), i420.height());
            let (data_y, data_u, data_v) = i420.data();
            let (stride_y, stride_u, stride_v) = i420.strides();
            let info = i420_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_u.as_ptr(),
                data_v.as_ptr(),
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false) }
        }
        VideoBufferType::I420 => {
            let i420 = rtcbuffer.as_i420().unwrap();
            let (width, height) = (i420.width(), i420.height());
            let (data_y, data_u, data_v) = i420.data();
            let (stride_y, stride_u, stride_v) = i420.strides();
            let info = i420_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_u.as_ptr(),
                data_v.as_ptr(),
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false) }
        }
        VideoBufferType::I420A => {
            let i420 = rtcbuffer.as_i420a().unwrap();
            let (width, height) = (i420.width(), i420.height());
            let (stride_y, stride_u, stride_v, stride_a) = i420.strides();
            let (data_y, data_u, data_v, data_a) = i420.data();
            let info = i420a_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_u.as_ptr(),
                data_v.as_ptr(),
                data_a.unwrap().as_ptr(),
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
                stride_a,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420a), false) }
        }
        VideoBufferType::I422 => {
            let i422 = rtcbuffer.as_i422().unwrap();
            let (width, height) = (i422.width(), i422.height());
            let (stride_y, stride_u, stride_v) = i422.strides();
            let (data_y, data_u, data_v) = i422.data();
            let info = i422_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_u.as_ptr(),
                data_v.as_ptr(),
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I422), false) }
        }
        VideoBufferType::I444 => {
            let i444 = rtcbuffer.as_i444().unwrap();
            let (width, height) = (i444.width(), i444.height());
            let (stride_y, stride_u, stride_v) = i444.strides();
            let (data_y, data_u, data_v) = i444.data();
            let info = i444_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_u.as_ptr(),
                data_v.as_ptr(),
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I444), false) }
        }
        VideoBufferType::I010 => {
            let i010 = rtcbuffer.as_i010().unwrap();
            let (width, height) = (i010.width(), i010.height());
            let (stride_y, stride_u, stride_v) = i010.strides();
            let (data_y, data_u, data_v) = i010.data();
            let info = i010_info(
                data_y.as_ptr() as *const u8,
                data_y.as_ptr() as *const u8,
                data_u.as_ptr() as *const u8,
                data_v.as_ptr() as *const u8,
                width,
                height,
                stride_y,
                stride_u,
                stride_v,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I010), false) }
        }
        VideoBufferType::NV12 => {
            let nv12 = rtcbuffer.as_nv12().unwrap();
            let (width, height) = (nv12.width(), nv12.height());
            let (stride_y, stride_uv) = nv12.strides();
            let (data_y, data_uv) = nv12.data();
            let info = nv12_info(
                data_y.as_ptr(),
                data_y.as_ptr(),
                data_uv.as_ptr(),
                width,
                height,
                stride_y,
                stride_uv,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::Nv12), false) }
        }
        _ => todo!(),
    }
}

pub fn i420_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_u: *const u8,
    data_v: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_u as u64,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_v as u64,
        stride: stride_v,
        size: stride_v * chroma_height,
    };
    components.extend_from_slice(&[c1, c2, c3]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I420.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn i420a_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_u: *const u8,
    data_v: *const u8,
    data_a: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    stride_a: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(4);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_u as u64,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_v as u64,
        stride: stride_v,
        size: stride_v * chroma_height,
    };

    let c4 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_a as u64,
        stride: stride_a,
        size: stride_a * height,
    };
    components.extend_from_slice(&[c1, c2, c3, c4]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I420a.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn i422_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_u: *const u8,
    data_v: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_u as u64,
        stride: stride_u,
        size: stride_u * height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_v as u64,
        stride: stride_v,
        size: stride_v * height,
    };
    components.extend_from_slice(&[c1, c2, c3]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I422.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn i444_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_u: *const u8,
    data_v: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_u as u64,
        stride: stride_u,
        size: stride_u * height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_v as u64,
        stride: stride_v,
        size: stride_v * height,
    };
    components.extend_from_slice(&[c1, c2, c3]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I444.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn i010_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_u: *const u8,
    data_v: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_u as u64,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_v as u64,
        stride: stride_v,
        size: stride_v * chroma_height,
    };
    components.extend_from_slice(&[c1, c2, c3]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::I010.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn nv12_info(
    data_ptr: *const u8,
    data_y: *const u8,
    data_uv: *const u8,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_uv: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(2);
    let c1 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_y as u64,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        data_ptr: data_uv as u64,
        stride: stride_uv,
        size: stride_uv * chroma_height * 2,
    };
    components.extend_from_slice(&[c1, c2]);

    proto::VideoBufferInfo {
        width,
        height,
        r#type: proto::VideoBufferType::Nv12.into(),
        components,
        data_ptr: data_ptr as u64,
        stride: 0,
    }
}

pub fn rgba_info(
    data_ptr: *const u8,
    r#type: proto::VideoBufferType,
    width: u32,
    height: u32,
) -> proto::VideoBufferInfo {
    proto::VideoBufferInfo {
        width,
        height,
        r#type: r#type.into(),
        components: Vec::default(),
        data_ptr: data_ptr as u64,
        stride: width * 4,
    }
}

pub fn rgb_info(
    data_ptr: *const u8,
    r#type: proto::VideoBufferType,
    width: u32,
    height: u32,
) -> proto::VideoBufferInfo {
    proto::VideoBufferInfo {
        width,
        height,
        r#type: r#type.into(),
        components: Vec::default(),
        data_ptr: data_ptr as u64,
        stride: width * 3,
    }
}
