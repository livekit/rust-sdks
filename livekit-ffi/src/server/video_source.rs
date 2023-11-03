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

use std::slice;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{BoxVideoBuffer, VideoFrame};

use super::FfiHandle;

pub struct FfiVideoSource {
    pub handle_id: FfiHandleId,
    pub source_type: proto::VideoSourceType,
    pub source: RtcVideoSource,
}

impl FfiHandle for FfiVideoSource {}

impl FfiVideoSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewVideoSourceRequest,
    ) -> FfiResult<proto::OwnedVideoSource> {
        let source_type = new_source.r#type();
        #[allow(unreachable_patterns)]
        let source_inner = match source_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoSourceType::VideoSourceNative => {
                use livekit::webrtc::video_source::native::NativeVideoSource;
                let video_source = NativeVideoSource::new(
                    new_source.resolution.map(Into::into).unwrap_or_default(),
                );
                RtcVideoSource::Native(video_source)
            }
            _ => {
                return Err(FfiError::InvalidRequest(
                    "unsupported video source type".into(),
                ))
            }
        };

        let handle_id = server.next_id();
        let video_source = Self {
            handle_id,
            source_type,
            source: source_inner,
        };
        let source_info = proto::VideoSourceInfo::from(&video_source);
        server.store_handle(handle_id, video_source);

        Ok(proto::OwnedVideoSource {
            handle: Some(proto::FfiOwnedHandle { id: handle_id }),
            info: Some(source_info),
        })
    }

    pub unsafe fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureVideoFrameRequest,
    ) -> FfiResult<()> {

        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(ref source) => {
                let frame_info = capture
                    .frame
                    .ok_or(FfiError::InvalidRequest("frame is empty".into()))?;

                let from = capture
                    .from
                    .ok_or(FfiError::InvalidRequest("capture from is empty".into()))?;

                // copy the provided buffer
                #[rustfmt::skip]
                let buffer: BoxVideoBuffer = match from {
                    proto::capture_video_frame_request::From::Info(info) => {
                        match &info.buffer {
                            Some(proto::video_frame_buffer_info::Buffer::Yuv(yuv)) => {
                                 match info.buffer_type() {
                                    proto::VideoFrameBufferType::I420 
                                        | proto::VideoFrameBufferType::I420a
                                        | proto::VideoFrameBufferType::I422
                                        | proto::VideoFrameBufferType::I444 => {

                                        let (y, u, v) = (
                                            slice::from_raw_parts(yuv.data_y_ptr as *const u8, (yuv.stride_y * info.height) as usize),
                                            slice::from_raw_parts(yuv.data_u_ptr as *const u8, (yuv.stride_u * yuv.chroma_height) as usize),
                                            slice::from_raw_parts(yuv.data_v_ptr as *const u8, (yuv.stride_v * yuv.chroma_height) as usize)
                                        );

                                        match info.buffer_type() {
                                            proto::VideoFrameBufferType::I420 | proto::VideoFrameBufferType::I420a => {
                                                let mut i420 = I420Buffer::with_strides(info.width, info.height, yuv.stride_y, yuv.stride_u, yuv.stride_v);
                                                let (dy, du, dv) = i420.data_mut();
                                                
                                                dy.copy_from_slice(y);
                                                du.copy_from_slice(u);
                                                dv.copy_from_slice(v);
                                                Box::new(i420) as BoxVideoBuffer
                                            },
                                            proto::VideoFrameBufferType::I422 => {
                                                let mut i422 = I422Buffer::with_strides(info.width, info.height, yuv.stride_y, yuv.stride_u, yuv.stride_v);
                                                let (dy, du, dv) = i422.data_mut();

                                                dy.copy_from_slice(y);
                                                du.copy_from_slice(u);
                                                dv.copy_from_slice(v);
                                                Box::new(i422) as BoxVideoBuffer
                                            },
                                            proto::VideoFrameBufferType::I444 => {
                                                let mut i444 = I444Buffer::with_strides(info.width, info.height, yuv.stride_y, yuv.stride_u, yuv.stride_v);
                                                let (dy, du, dv) = i444.data_mut();

                                                dy.copy_from_slice(y);
                                                du.copy_from_slice(u);
                                                dv.copy_from_slice(v);
                                                Box::new(i444) as BoxVideoBuffer
                                            }
                                            _ => unreachable!()
                                        }
                                   }
                                   proto::VideoFrameBufferType::I010 => {
                                        let (y, u, v) = (
                                            slice::from_raw_parts(yuv.data_y_ptr as *const u16, (yuv.stride_y * info.height) as usize / std::mem::size_of::<u16>()),
                                            slice::from_raw_parts(yuv.data_u_ptr as *const u16, (yuv.stride_u * yuv.chroma_height) as usize / std::mem::size_of::<u16>()),
                                            slice::from_raw_parts(yuv.data_v_ptr as *const u16, (yuv.stride_v * yuv.chroma_height) as usize / std::mem::size_of::<u16>())
                                        );

                                        let mut i010 = I010Buffer::with_strides(info.width, info.height, yuv.stride_y, yuv.stride_u, yuv.stride_v);
                                        let (dy, du, dv) = i010.data_mut();

                                        dy.copy_from_slice(y);
                                        du.copy_from_slice(u);
                                        dv.copy_from_slice(v);
                                        Box::new(i010) as BoxVideoBuffer
                                    }
                                    _ => return Err(FfiError::InvalidRequest("invalid yuv description".into()))
                                }
                            }
                            Some(proto::video_frame_buffer_info::Buffer::BiYuv(biyuv)) => {
                                let (y, uv) = (
                                    slice::from_raw_parts(biyuv.data_y_ptr as *const u8, (biyuv.stride_y * info.height) as usize),
                                    slice::from_raw_parts(biyuv.data_uv_ptr as *const u8, (biyuv.stride_uv * biyuv.chroma_height) as usize)
                                );

                                if info.buffer_type() == proto::VideoFrameBufferType::Nv12 {
                                    let mut nv12 = NV12Buffer::with_strides(info.width, info.height, biyuv.stride_y, biyuv.stride_uv);
                                    let (dy, duv) = nv12.data_mut();

                                    dy.copy_from_slice(y);
                                    duv.copy_from_slice(uv);
                                    Box::new(nv12) as BoxVideoBuffer 
                                } else {
                                    return Err(FfiError::InvalidRequest("invalid biyuv description".into()))
                                }
                           }
                            _ => return Err(FfiError::InvalidRequest("conversion not supported".into()))
                        }
                    }
                    proto::capture_video_frame_request::From::Handle(handle) => {
                        let (_, buffer) = server
                            .ffi_handles
                            .remove(&handle)
                            .ok_or(FfiError::InvalidRequest("handle not found".into()))?;

                        *(buffer
                            .downcast::<BoxVideoBuffer>()
                            .map_err(|_| FfiError::InvalidRequest("handle is not video frame".into()))?)
                    }
                };

                let frame = VideoFrame {
                    rotation: frame_info.rotation().into(),
                    timestamp_us: frame_info.timestamp_us,
                    buffer,
                };

                source.capture_frame(&frame);
            }
            _ => {}
        }
        Ok(())
    }
}
