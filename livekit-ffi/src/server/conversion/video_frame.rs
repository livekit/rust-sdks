use crate::server::FFIHandleId;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame;
use livekit_protocol as proto;
use std::any::Any;

macro_rules! impl_yuv_into {
    (@fields, $buffer:ident, $data_y:ident, $data_u:ident, $data_v: ident) => {
        Self {
            chroma_width: $buffer.chroma_width(),
            chroma_height: $buffer.chroma_height(),
            stride_y: $buffer.stride_y(),
            stride_u: $buffer.stride_u(),
            stride_v: $buffer.stride_v(),
            data_y_ptr: $data_y.as_ptr() as u64,
            data_u_ptr: $data_u.as_ptr() as u64,
            data_v_ptr: $data_v.as_ptr() as u64,
            ..Default::default()
        }
    };
    ($buffer:ty, ALPHA) => {
        impl From<$buffer> for proto::PlanarYuvBufferInfo {
            fn from(buffer: $buffer) -> Self {
                let (data_y, data_u, data_v, data_a) = buffer.data();
                let mut proto = impl_yuv_into!(@fields, buffer, data_y, data_u, data_v);
                proto.stride_a = buffer.stride_a();
                proto.data_a_ptr = data_a.map(|data_a| data_a.as_ptr() as u64).unwrap_or(0);
                proto
            }
        }
    };
    ($buffer:ty) => {
        impl From<$buffer> for proto::PlanarYuvBufferInfo {
            fn from(buffer: $buffer) -> Self {
                let (data_y, data_u, data_v) = buffer.data();
                impl_yuv_into!(@fields, buffer, data_y, data_u, data_v)
            }
        }
    };
}

macro_rules! impl_biyuv_into {
    ($b:ty) => {
        impl From<$b> for proto::BiplanarYuvBufferInfo {
            fn from(buffer: $b) -> Self {
                let (data_y, data_uv) = buffer.data();
                Self {
                    chroma_width: buffer.chroma_width(),
                    chroma_height: buffer.chroma_height(),
                    stride_y: buffer.stride_y(),
                    stride_uv: buffer.stride_uv(),
                    data_y_ptr: data_y.as_ptr() as u64,
                    data_uv_ptr: data_uv.as_ptr() as u64,
                }
            }
        }
    };
}

impl_yuv_into!(&I420Buffer);
impl_yuv_into!(&I420ABuffer, ALPHA);
impl_yuv_into!(&I422Buffer);
impl_yuv_into!(&I444Buffer);
impl_yuv_into!(&I010Buffer);
impl_biyuv_into!(&NV12Buffer);

impl proto::VideoFrameInfo {
    pub fn from<T>(frame: &VideoFrame<T>) -> Self
    where
        T: VideoFrameBuffer,
    {
        Self {
            timestamp: frame.timestamp,
            rotation: proto::VideoRotation::from(frame.rotation).into(),
        }
    }
}

impl proto::VideoFrameBufferInfo {
    pub fn from(handle: FFIHandleId, buffer: &dyn VideoFrameBuffer) -> Self {
        match &buffer.buffer_type() {
            #[cfg(not(target_arch = "wasm32"))]
            VideoFrameBufferType::Native => Self::from_native(handle, buffer.as_native().unwrap()),
            VideoFrameBufferType::I420 => Self::from_i420(handle, buffer.as_i420().unwrap()),
            VideoFrameBufferType::I420A => Self::from_i420a(handle, buffer.as_i420a().unwrap()),
            VideoFrameBufferType::I422 => Self::from_i422(handle, buffer.as_i422().unwrap()),
            VideoFrameBufferType::I444 => Self::from_i444(handle, buffer.as_i444().unwrap()),
            VideoFrameBufferType::I010 => Self::from_i010(handle, buffer.as_i010().unwrap()),
            VideoFrameBufferType::NV12 => Self::from_nv12(handle, buffer.as_nv12().unwrap()),
            _ => panic!("unsupported buffer type on this platform"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_native(handle_id: FFIHandleId, buffer: &video_frame::native::NativeBuffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::Native.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Native(
                proto::NativeBufferInfo {},
            )),
        }
    }

    pub fn from_i420(handle_id: FFIHandleId, buffer: &I420Buffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::I420.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(buffer.into())),
        }
    }

    pub fn from_i420a(handle_id: FFIHandleId, buffer: &I420ABuffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::I420a.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(buffer.into())),
        }
    }

    pub fn from_i422(handle_id: FFIHandleId, buffer: &I422Buffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::I422.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(buffer.into())),
        }
    }

    pub fn from_i444(handle_id: FFIHandleId, buffer: &I444Buffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::I444.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(buffer.into())),
        }
    }

    pub fn from_i010(handle_id: FFIHandleId, buffer: &I010Buffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::I010.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(buffer.into())),
        }
    }

    pub fn from_nv12(handle_id: FFIHandleId, buffer: &NV12Buffer) -> Self {
        Self {
            handle: Some(handle_id.into()),
            buffer_type: proto::VideoFrameBufferType::Nv12.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::BiYuv(buffer.into())),
        }
    }
}

impl From<proto::VideoFormatType> for VideoFormatType {
    fn from(format: proto::VideoFormatType) -> Self {
        match format {
            proto::VideoFormatType::FormatArgb => Self::ARGB,
            proto::VideoFormatType::FormatBgra => Self::BGRA,
            proto::VideoFormatType::FormatAbgr => Self::ABGR,
            proto::VideoFormatType::FormatRgba => Self::RGBA,
        }
    }
}

impl From<VideoRotation> for proto::VideoRotation {
    fn from(rotation: VideoRotation) -> proto::VideoRotation {
        match rotation {
            VideoRotation::VideoRotation0 => Self::VideoRotation0,
            VideoRotation::VideoRotation90 => Self::VideoRotation90,
            VideoRotation::VideoRotation180 => Self::VideoRotation180,
            VideoRotation::VideoRotation270 => Self::VideoRotation270,
        }
    }
}

impl From<VideoFrameBufferType> for proto::VideoFrameBufferType {
    fn from(buffer_type: VideoFrameBufferType) -> Self {
        match buffer_type {
            VideoFrameBufferType::Native => Self::Native,
            VideoFrameBufferType::I420 => Self::I420,
            VideoFrameBufferType::I420A => Self::I420a,
            VideoFrameBufferType::I422 => Self::I422,
            VideoFrameBufferType::I444 => Self::I444,
            VideoFrameBufferType::I010 => Self::I010,
            VideoFrameBufferType::NV12 => Self::Nv12,
            VideoFrameBufferType::WebGl => Self::Webgl,
            _ => panic!("unsupported buffer type on FFI server"),
        }
    }
}
