use crate::proto;
use livekit::webrtc::{prelude::*, video_frame::BoxVideoBuffer};
use std::slice;

pub mod cvtimpl;

macro_rules! to_i420 {
    ($buffer:ident, $data:expr) => {{
        let proto::VideoBufferInfo { width, height, components, .. } = $buffer;
        let [c0, c1, c2, ..] = components.as_slice();
        let (y, u, v) = split_i420($data, c0.stride, c1.stride, c2.stride, height);
        let mut i420 = I420Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

        let (dy, du, dv) = i420.data_mut();
        dy.copy_from_slice(y);
        du.copy_from_slice(u);
        dv.copy_from_slice(v);
        Box::new(i420) as BoxVideoBuffer
    }};
}

pub fn to_libwebrtc_buffer(info: proto::VideoBufferInfo) -> BoxVideoBuffer {
    let data = unsafe { slice::from_raw_parts(info.data_ptr as *const u8, info.data_len as usize) };

    let proto::VideoBufferInfo { width, height, components, .. } = info;

    match info.r#type() {
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
            to_i420!(info, data)
        }
        proto::VideoBufferType::I422 => {
            let [c0, c1, c2, ..] = components.as_slice();
            let (y, u, v) = split_i422(data, c0.stride, c1.stride, c2.stride, height);
            let mut i422 = I422Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i422.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i422) as BoxVideoBuffer
        }
        proto::VideoBufferType::I444 => {
            let [c0, c1, c2, ..] = components.as_slice();
            let (y, u, v) = split_i444(data, c0.stride, c1.stride, c2.stride, height);
            let mut i444 = I444Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i444.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i444) as BoxVideoBuffer
        }
        proto::VideoBufferType::I010 => {
            let [c0, c1, c2, ..] = components.as_slice();
            let (y, u, v) = split_i010(data, c0.stride, c1.stride, c2.stride, height);

            let (_, y, _) = unsafe { y.align_to_mut::<u16>() };
            let (_, u, _) = unsafe { u.align_to_mut::<u16>() };
            let (_, v, _) = unsafe { v.align_to_mut::<u16>() };

            let mut i010 = I010Buffer::with_strides(width, height, c0.stride, c1.stride, c2.stride);

            let (dy, du, dv) = i010.data_mut();
            dy.copy_from_slice(y);
            du.copy_from_slice(u);
            dv.copy_from_slice(v);
            Box::new(i010) as BoxVideoBuffer
        }
        proto::VideoBufferType::Nv12 => {
            let [c0, c1, ..] = components.as_slice();
            let (y, uv) = split_nv12(data, c0.stride, c1.stride, info.height);
            let mut nv12 = NV12Buffer::with_strides(info.width, info.height, c0.stride, c1.stride);

            let (dy, duv) = nv12.data_mut();
            dy.copy_from_slice(y);
            duv.copy_from_slice(uv);
            Box::new(nv12) as BoxVideoBuffer
        }
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
    let (luma, chroma) = dst.split_at_mut((stride_y * height) as usize);
    let (u, v) = chroma.split_at_mut((stride_u * chroma_height) as usize);
    (luma, u, v)
}

pub fn split_i420a(
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

pub fn split_i422(
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

pub fn split_i444(
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

pub fn split_i010(
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

pub fn split_nv12(src: &[u8], stride_y: u32, stride_uv: u32, height: u32) -> (&[u8], &[u8]) {
    let chroma_height = (height + 1) / 2;
    let (luma, chroma) = src.split_at((stride_y * height) as usize);
    let (u, v) = chroma.split_at((stride_uv * chroma_height) as usize);
    (luma, u)
}

pub fn i420_info(data: &[u8], width: u32, height: u32) -> proto::VideoBufferInfo {
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
