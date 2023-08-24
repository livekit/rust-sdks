// Copyright 2023 LiveKit, Inc.
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

use crate::proto;
use crate::server::video_source::FfiVideoSource;
use crate::server::video_stream::FfiVideoStream;
use livekit::options::{VideoCodec, VideoResolution};
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame;
use livekit::webrtc::video_source::VideoResolution as VideoSourceResolution;

impl From<proto::VideoSourceResolution> for VideoSourceResolution {
    fn from(res: proto::VideoSourceResolution) -> Self {
        Self {
            width: res.width,
            height: res.height,
        }
    }
}

impl proto::VideoFrameInfo {
    pub fn from<T>(frame: &VideoFrame<T>) -> Self
    where
        T: AsRef<dyn VideoFrameBuffer>,
    {
        Self {
            timestamp_us: frame.timestamp_us,
            rotation: proto::VideoRotation::from(frame.rotation).into(),
        }
    }
}

impl From<&FfiVideoSource> for proto::VideoSourceInfo {
    fn from(source: &FfiVideoSource) -> Self {
        Self {
            r#type: source.source_type as i32,
        }
    }
}

impl From<&FfiVideoStream> for proto::VideoStreamInfo {
    fn from(stream: &FfiVideoStream) -> Self {
        Self {
            r#type: stream.stream_type as i32,
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

impl From<proto::VideoRotation> for VideoRotation {
    fn from(rotation: proto::VideoRotation) -> VideoRotation {
        match rotation {
            proto::VideoRotation::VideoRotation0 => Self::VideoRotation0,
            proto::VideoRotation::VideoRotation90 => Self::VideoRotation90,
            proto::VideoRotation::VideoRotation180 => Self::VideoRotation180,
            proto::VideoRotation::VideoRotation270 => Self::VideoRotation270,
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

impl From<VideoResolution> for proto::VideoResolution {
    fn from(resolution: VideoResolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
            frame_rate: resolution.frame_rate,
        }
    }
}

impl From<proto::VideoResolution> for VideoResolution {
    fn from(resolution: proto::VideoResolution) -> Self {
        Self {
            width: resolution.width,
            height: resolution.height,
            frame_rate: resolution.frame_rate,
            aspect_ratio: resolution.width as f32 / resolution.height as f32,
        }
    }
}

impl From<proto::VideoCodec> for VideoCodec {
    fn from(codec: proto::VideoCodec) -> Self {
        match codec {
            proto::VideoCodec::Vp8 => Self::VP8,
            proto::VideoCodec::H264 => Self::H264,
            proto::VideoCodec::Av1 => Self::AV1,
        }
    }
}

macro_rules! impl_yuv_into {
    (@fields, $buffer:ident, $data_y:ident, $data_u:ident, $data_v: ident) => {
        Self {
            chroma_width: $buffer.chroma_width(),
            chroma_height: $buffer.chroma_height(),
            stride_y: $buffer.strides().0,
            stride_u: $buffer.strides().1,
            stride_v: $buffer.strides().2,
            data_y_ptr: $data_y.as_ptr() as u64,
            data_u_ptr: $data_u.as_ptr() as u64,
            data_v_ptr: $data_v.as_ptr() as u64,
            ..Default::default()
        }
    };
    ($fncname:ident, $buffer:ty, ALPHA) => {
        fn $fncname(buffer: $buffer) -> Self {
            let (data_y, data_u, data_v, data_a) = buffer.data();
            let mut proto = impl_yuv_into!(@fields, buffer, data_y, data_u, data_v);
            proto.stride_a = buffer.strides().3;
            proto.data_a_ptr = data_a.map(|data_a| data_a.as_ptr() as u64).unwrap_or(0);
            proto
        }
    };
    ($fncname:ident, $buffer:ty) => {
        fn $fncname(buffer: $buffer) -> Self {
            let (data_y, data_u, data_v) = buffer.data();
            impl_yuv_into!(@fields, buffer, data_y, data_u, data_v)
        }
    };
}

macro_rules! impl_biyuv_into {
    ($fncname:ident, $buffer:ty) => {
        fn $fncname(buffer: $buffer) -> Self {
            let (stride_y, stride_uv) = buffer.strides();
            let (data_y, data_uv) = buffer.data();
            Self {
                chroma_width: buffer.chroma_width(),
                chroma_height: buffer.chroma_height(),
                stride_y,
                stride_uv,
                data_y_ptr: data_y.as_ptr() as u64,
                data_uv_ptr: data_uv.as_ptr() as u64,
            }
        }
    };
}

impl proto::PlanarYuvBufferInfo {
    impl_yuv_into!(from_i420, &I420Buffer);
    impl_yuv_into!(from_i420a, &I420ABuffer, ALPHA);
    impl_yuv_into!(from_i422, &I422Buffer);
    impl_yuv_into!(from_i444, &I444Buffer);
    impl_yuv_into!(from_i010, &I010Buffer);
}

impl proto::BiplanarYuvBufferInfo {
    impl_biyuv_into!(from_nv12, &NV12Buffer);
}

impl proto::VideoFrameBufferInfo {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_native(buffer: &video_frame::native::NativeBuffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::Native.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Native(
                proto::NativeBufferInfo {},
            )),
        }
    }

    fn from_i420(buffer: &I420Buffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::I420.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(
                proto::PlanarYuvBufferInfo::from_i420(buffer),
            )),
        }
    }

    fn from_i420a(buffer: &I420ABuffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::I420a.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(
                proto::PlanarYuvBufferInfo::from_i420a(buffer),
            )),
        }
    }

    fn from_i422(buffer: &I422Buffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::I422.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(
                proto::PlanarYuvBufferInfo::from_i422(buffer),
            )),
        }
    }

    fn from_i444(buffer: &I444Buffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::I444.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(
                proto::PlanarYuvBufferInfo::from_i444(buffer),
            )),
        }
    }

    fn from_i010(buffer: &I010Buffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::I010.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::Yuv(
                proto::PlanarYuvBufferInfo::from_i010(buffer),
            )),
        }
    }

    fn from_nv12(buffer: &NV12Buffer) -> Self {
        Self {
            buffer_type: proto::VideoFrameBufferType::Nv12.into(),
            width: buffer.width(),
            height: buffer.height(),
            buffer: Some(proto::video_frame_buffer_info::Buffer::BiYuv(
                proto::BiplanarYuvBufferInfo::from_nv12(buffer),
            )),
        }
    }
}

impl<B: AsRef<dyn VideoFrameBuffer>> From<B> for proto::VideoFrameBufferInfo {
    fn from(buffer: B) -> Self {
        let buffer = buffer.as_ref();
        match buffer.buffer_type() {
            #[cfg(not(target_arch = "wasm32"))]
            VideoFrameBufferType::Native => Self::from_native(buffer.as_native().unwrap()),
            VideoFrameBufferType::I420 => Self::from_i420(buffer.as_i420().unwrap()),
            VideoFrameBufferType::I420A => Self::from_i420a(buffer.as_i420a().unwrap()),
            VideoFrameBufferType::I422 => Self::from_i422(buffer.as_i422().unwrap()),
            VideoFrameBufferType::I444 => Self::from_i444(buffer.as_i444().unwrap()),
            VideoFrameBufferType::I010 => Self::from_i010(buffer.as_i010().unwrap()),
            VideoFrameBufferType::NV12 => Self::from_nv12(buffer.as_nv12().unwrap()),
            _ => panic!("unsupported buffer type on this platform"),
        }
    }
}
