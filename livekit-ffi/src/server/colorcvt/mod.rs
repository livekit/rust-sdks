use crate::{proto, FfiResult};
use livekit::webrtc::{prelude::*, video_frame::BoxVideoBuffer};
use std::slice;

pub mod cvtimpl;

macro_rules! to_i420 {
    ($buffer:ident, $data:expr) => {{
        let proto::VideoBufferInfo { width, height, components, .. } = $buffer;
        let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
        let (y, u, v) = split_i420($data, c0.stride, c1.stride, c2.stride, height);
        let mut i420 = I420Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

        let (dy, du, dv) = i420.data_mut();
        dy.copy_from_slice(y);
        du.copy_from_slice(u);
        dv.copy_from_slice(v);
        Box::new(i420) as BoxVideoBuffer
    }};
}

pub unsafe fn to_libwebrtc_buffer(info: proto::VideoBufferInfo) -> BoxVideoBuffer {
    let data = unsafe { slice::from_raw_parts(info.data_ptr as *const u8, info.data_len as usize) };
    let r#type = info.r#type();
    let proto::VideoBufferInfo { width, height, components, .. } = info.clone();

    match r#type {
        // For rgba buffer, automatically convert to I420
        proto::VideoBufferType::Rgba => {
            let (data, info) =
                cvtimpl::cvt_rgba(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info, &data)
        }
        proto::VideoBufferType::Abgr => {
            let (data, info) =
                cvtimpl::cvt_abgr(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info, &data)
        }
        proto::VideoBufferType::Argb => {
            let (data, info) =
                cvtimpl::cvt_argb(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info, &data)
        }
        proto::VideoBufferType::Bgra => {
            let (data, info) =
                cvtimpl::cvt_bgra(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info, &data)
        }
        proto::VideoBufferType::Rgb24 => {
            let (data, info) =
                cvtimpl::cvt_rgb24(info, proto::VideoBufferType::I420, false).unwrap();
            to_i420!(info, &data)
        }
        proto::VideoBufferType::I420 | proto::VideoBufferType::I420a => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
            let (y, u, v) = split_i420(data, c0.stride, c1.stride, c2.stride, height);
            let mut i420 = I420Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i420.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i420) as BoxVideoBuffer
        }
        proto::VideoBufferType::I422 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
            let (y, u, v) = split_i422(data, c0.stride, c1.stride, c2.stride, height);
            let mut i422 = I422Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i422.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i422) as BoxVideoBuffer
        }
        proto::VideoBufferType::I444 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
            let (y, u, v) = split_i444(data, c0.stride, c1.stride, c2.stride, height);
            let mut i444 = I444Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i444.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i444) as BoxVideoBuffer
        }
        proto::VideoBufferType::I010 => {
            let (c0, c1, c2) = (&components[0], &components[1], &components[2]);
            let (y, u, v) = split_i010(data, c0.stride, c1.stride, c2.stride, height);

            let (_, y, _) = unsafe { y.align_to::<u16>() };
            let (_, u, _) = unsafe { u.align_to::<u16>() };
            let (_, v, _) = unsafe { v.align_to::<u16>() };

            let mut i010 = I010Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i010.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i010) as BoxVideoBuffer
        }
        proto::VideoBufferType::Nv12 => {
            let (c0, c1) = (&components[0], &components[1]);
            let (y, uv) = split_nv12(data, c0.stride, c1.stride, info.height);
            let mut nv12 = NV12Buffer::with_strides(info.width, info.height, c0.stride, c1.stride);

            let (dy, duv) = nv12.data_mut();
            dy.copy_from_slice(y);
            duv.copy_from_slice(uv);
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
            let (stride_y, stride_u, stride_v) = i420.strides();
            let ptr = i420.data().0.as_ptr() as *const u8;
            let len = (stride_y * i420.height()
                + stride_u * i420.chroma_height()
                + stride_v * i420.chroma_height()) as usize;
            let info =
                i420_info(ptr, len, i420.width(), i420.height(), stride_y, stride_u, stride_v);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false) }
        }
        VideoBufferType::I420 => {
            let i420 = rtcbuffer.as_i420().unwrap();
            let (stride_y, stride_u, stride_v) = i420.strides();
            let ptr = i420.data().0.as_ptr() as *const u8;
            let len = (stride_y * i420.height()
                + stride_u * i420.chroma_height()
                + stride_v * i420.chroma_height()) as usize;
            let info =
                i420_info(ptr, len, i420.width(), i420.height(), stride_y, stride_u, stride_v);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false) }
        }
        VideoBufferType::I420A => {
            let i420 = rtcbuffer.as_i420a().unwrap();
            let (stride_y, stride_u, stride_v, stride_a) = i420.strides();
            let ptr = i420.data().0.as_ptr() as *const u8;
            let len = (stride_y * i420.height()
                + stride_u * i420.chroma_height()
                + stride_v * i420.chroma_height()
                + stride_a * i420.height()) as usize;

            let info = i420a_info(
                ptr,
                len,
                i420.width(),
                i420.height(),
                stride_y,
                stride_u,
                stride_v,
                stride_a,
            );
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420a), false) }
        }
        VideoBufferType::I422 => {
            let i422 = rtcbuffer.as_i422().unwrap();
            let (stride_y, stride_u, stride_v) = i422.strides();
            let ptr = i422.data().0.as_ptr() as *const u8;
            let len = (stride_y * i422.height()
                + stride_u * i422.chroma_height()
                + stride_v * i422.chroma_height()) as usize;
            let info =
                i422_info(ptr, len, i422.width(), i422.height(), stride_y, stride_u, stride_v);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I422), false) }
        }
        VideoBufferType::I444 => {
            let i444 = rtcbuffer.as_i444().unwrap();
            let (stride_y, stride_u, stride_v) = i444.strides();
            let ptr = i444.data().0.as_ptr() as *const u8;
            let len = (stride_y * i444.height()
                + stride_u * i444.chroma_height()
                + stride_v * i444.chroma_height()) as usize;
            let info =
                i444_info(ptr, len, i444.width(), i444.height(), stride_y, stride_u, stride_v);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I444), false) }
        }
        VideoBufferType::I010 => {
            let i010 = rtcbuffer.as_i010().unwrap();
            let (stride_y, stride_u, stride_v) = i010.strides();
            let ptr = i010.data().0.as_ptr() as *const u8;
            let len = (stride_y * i010.height()
                + stride_u * i010.chroma_height()
                + stride_v * i010.chroma_height()) as usize;
            let info =
                i010_info(ptr, len, i010.width(), i010.height(), stride_y, stride_u, stride_v);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I010), false) }
        }
        VideoBufferType::NV12 => {
            let nv12 = rtcbuffer.as_nv12().unwrap();
            let (stride_y, stride_uv) = nv12.strides();
            let ptr = nv12.data().0.as_ptr() as *const u8;
            let len = stride_y * nv12.height() + stride_uv * nv12.chroma_height() * 2;
            let info =
                nv12_info(ptr, len as usize, nv12.width(), nv12.height(), stride_y, stride_uv);
            unsafe { cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::Nv12), false) }
        }
        _ => todo!(),
    }
}

pub fn split_i420_mut(
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

pub fn split_i420(
    dst: &[u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = dst.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * chroma_height) as usize);
    (luma, u, v)
}

pub fn split_i420a(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    _stride_a: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * chroma_height) as usize);
    let (v, a) = v.split_at((stride_v * chroma_height) as usize);
    (luma, u, v, a)
}

pub fn split_i420a_mut(
    src: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    _stride_a: u32,
    height: u32,
) -> (&mut [u8], &mut [u8], &mut [u8], &mut [u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * chroma_height) as usize);
    let (v, a) = v.split_at_mut((stride_v * chroma_height) as usize);
    (luma, u, v, a)
}

pub fn split_i422(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * height) as usize);
    (luma, u, v)
}

pub fn split_i422_mut(
    src: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&mut [u8], &mut [u8], &mut [u8]) {
    let (luma, chroma) = src.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * height) as usize);
    (luma, u, v)
}

pub fn split_i444(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * height) as usize);
    (luma, u, v)
}

pub fn split_i444_mut(
    src: &mut [u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&mut [u8], &mut [u8], &mut [u8]) {
    let (luma, chroma) = src.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * height) as usize);
    (luma, u, v)
}

pub fn split_i010(
    src: &[u8],
    stride_y: u32,
    stride_u: u32,
    _stride_v: u32,
    height: u32,
) -> (&[u8], &[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_u * chroma_height) as usize);
    (luma, u, v)
}

pub fn split_i010_mut(
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

pub fn split_nv12(src: &[u8], stride_y: u32, _stride_uv: u32, height: u32) -> (&[u8], &[u8]) {
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    (luma, chroma)
}

pub fn split_nv12_mut(
    src: &mut [u8],
    stride_y: u32,
    _stride_uv: u32,
    height: u32,
) -> (&mut [u8], &mut [u8]) {
    let (luma, chroma) = src.split_at_mut((stride_y * height) as usize);
    (luma, chroma)
}

pub fn i420_info(
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn i420a_info(
    data_ptr: *const u8,
    data_len: usize,
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
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
        stride: stride_v,
        size: stride_v * chroma_height,
    };

    let c4 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size + c3.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn i422_info(
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: stride_u,
        size: stride_u * height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn i444_info(
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: stride_u,
        size: stride_u * height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn i010_info(
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(3);
    let c1 = proto::video_buffer_info::ComponentInfo {
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
        stride: stride_u,
        size: stride_u * chroma_height,
    };

    let c3 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size + c2.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn nv12_info(
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_uv: u32,
) -> proto::VideoBufferInfo {
    let chroma_height = (height + 1) / 2;

    let mut components = Vec::with_capacity(2);
    let c1 = proto::video_buffer_info::ComponentInfo {
        offset: 0,
        stride: stride_y,
        size: stride_y * height,
    };

    let c2 = proto::video_buffer_info::ComponentInfo {
        offset: c1.size,
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
        data_len: data_len as u32,
        stride: 0,
    }
}

pub fn rgba_info(
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

pub fn rgb_info(
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
