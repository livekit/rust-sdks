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

use crate::{proto, FfiResult};
use livekit::webrtc::{prelude::*, video_frame::BoxVideoBuffer};
use std::slice;

pub mod cvtimpl;

pub unsafe fn to_libwebrtc_buffer(info: proto::VideoBufferInfo) -> BoxVideoBuffer {
    let r#type = info.r#type();
    let proto::VideoBufferInfo { width, height, components, .. } = info.clone();

    match r#type {
        // For RGBA-family / RGB24 buffers, fused conversion directly into I420Buffer
        proto::VideoBufferType::Rgba => cvtimpl::cvt_rgba_to_i420_buffer(info, false),
        proto::VideoBufferType::Abgr => cvtimpl::cvt_abgr_to_i420_buffer(info, false),
        proto::VideoBufferType::Argb => cvtimpl::cvt_argb_to_i420_buffer(info, false),
        proto::VideoBufferType::Bgra => cvtimpl::cvt_bgra_to_i420_buffer(info, false),
        proto::VideoBufferType::Rgb24 => cvtimpl::cvt_rgb24_to_i420_buffer(info, false),
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

/// Converts a libwebrtc frame buffer into `dst`, tightly packed, and returns
/// the pointers describing it.
///
/// `dst` belongs to the caller so a stream can hand the same allocation back
/// frame after frame instead of churning one frame-sized block per frame. The
/// returned info points into `dst`, so it is only valid while `dst` is.
///
/// TODO(sxian): skip the copy entirely when no conversion was asked for.
///
/// For a native-format request `cvt` only repacks the decoder's padded strides
/// into tight ones. We could hand out the decoder's buffer instead: the handle
/// map can already own one (`impl FfiHandle for BoxVideoBuffer`) and
/// `VideoBufferInfo.components` already carries per-plane stride. Two steps:
///
/// 1. Strides already tight (`stride == width`): the info emitted is identical
///    to what the copy produces, so no binding needs to change.
/// 2. Padded strides: needs stride-aware consumers. The C++ SDK reads
///    `components[].stride` already; Python's `VideoFrame` and Unity's
///    `YuvToRgbConverter` both assume tightly packed data.
///
/// That would also delete the stream's `FrameBuffers`: libwebrtc's
/// `VideoFrameBufferPool` already pools and refcounts decoder buffers, so this
/// layer is only pooling because it copies. Not avoidable when the application
/// requests a different pixel format, or for hardware decode where the source
/// is a platform texture.
pub fn to_video_buffer_info(
    rtcbuffer: BoxVideoBuffer,
    dst_type: Option<proto::VideoBufferType>,
    _normalize_stride: bool, // always normalize stride for now..
    dst: &mut Vec<u8>,
) -> FfiResult<proto::VideoBufferInfo> {
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
            // SAFETY: `info` holds raw pointers into `i420`, which `to_i420` returned
            // by value. It lives until the end of this arm, so the conversion has to
            // happen here rather than after the match.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `i420`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `i420`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I420a), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `i422`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I422), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `i444`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I444), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `i010`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::I010), false, dst)
            }
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
            // SAFETY: `info` holds raw pointers into `nv12`, borrowed from
            // `rtcbuffer`, which outlives this call.
            unsafe {
                cvtimpl::cvt(info, dst_type.unwrap_or(proto::VideoBufferType::Nv12), false, dst)
            }
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
        stride: None,
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
        stride: None,
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
        stride: None,
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
        stride: None,
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
        stride: None,
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
        stride: None,
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
        stride: Some(width * 4),
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
        stride: Some(width * 3),
    }
}

#[cfg(test)]
mod tests {
    use livekit::webrtc::video_frame::I420Buffer;

    use super::*;

    /// A realistic shape: 640x480 I420 is 460,800 bytes.
    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 480;

    fn i420_frame(width: u32, height: u32) -> BoxVideoBuffer {
        Box::new(I420Buffer::new(width, height)) as BoxVideoBuffer
    }

    fn convert_into(dst: &mut Vec<u8>, width: u32, height: u32) -> proto::VideoBufferInfo {
        to_video_buffer_info(
            i420_frame(width, height),
            Some(proto::VideoBufferType::I420),
            true,
            dst,
        )
        .expect("i420 to i420 is supported")
    }

    #[test]
    fn conversion_writes_into_the_buffer_it_is_given() {
        let mut dst = Vec::new();

        let info = convert_into(&mut dst, WIDTH, HEIGHT);

        let chroma_w = (WIDTH + 1) / 2;
        let chroma_h = (HEIGHT + 1) / 2;
        assert_eq!(dst.len(), (WIDTH * HEIGHT + chroma_w * chroma_h * 2) as usize);
        assert_eq!(info.data_ptr, dst.as_ptr() as u64, "the info points into the caller's buffer");
    }

    #[test]
    fn converting_again_into_the_same_buffer_does_not_reallocate() {
        // This is what lets a stream hand the same allocation back frame after
        // frame instead of churning one per frame.
        let mut dst = Vec::new();
        convert_into(&mut dst, WIDTH, HEIGHT);
        let address = dst.as_ptr();

        let info = convert_into(&mut dst, WIDTH, HEIGHT);

        assert_eq!(dst.as_ptr(), address, "the second frame reused the allocation");
        assert_eq!(info.data_ptr, address as u64);
    }

    #[test]
    fn a_different_frame_size_resizes_the_buffer() {
        // A simulcast layer switch changes the frame size mid-stream.
        let mut dst = Vec::new();
        convert_into(&mut dst, WIDTH, HEIGHT);

        let info = convert_into(&mut dst, WIDTH / 2, HEIGHT / 2);

        let chroma_w = (WIDTH / 2 + 1) / 2;
        let chroma_h = (HEIGHT / 2 + 1) / 2;
        assert_eq!(dst.len(), ((WIDTH / 2) * (HEIGHT / 2) + chroma_w * chroma_h * 2) as usize);
        assert_eq!(info.width, WIDTH / 2);
        assert_eq!(info.data_ptr, dst.as_ptr() as u64);
    }

    #[test]
    fn conversion_output_is_tightly_packed() {
        let mut dst = Vec::new();

        let info = convert_into(&mut dst, WIDTH, HEIGHT);

        let chroma_w = (WIDTH + 1) / 2;
        assert_eq!(info.width, WIDTH);
        assert_eq!(info.height, HEIGHT);
        let [y, u, v] = &info.components[..] else { panic!("expected three planes") };
        assert_eq!((y.stride, u.stride, v.stride), (WIDTH, chroma_w, chroma_w));
    }
}
