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

use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{BoxVideoFrameBuffer, VideoFrame};

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

    pub fn capture_frame(
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
                let buffer: &BoxVideoFrameBuffer = match from {
                    proto::capture_video_frame_request::From::Info(info) => {
                        unreachable!()
                    }
                    proto::capture_video_frame_request::From::Handle(handle) => {
                        let buffer = server
                            .ffi_handles
                            .get(&handle)
                            .ok_or(FfiError::InvalidRequest("handle not found".into()))?;

                        buffer
                            .downcast_ref::<BoxVideoFrameBuffer>()
                            .ok_or(FfiError::InvalidRequest("handle is not video frame".into()))?
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
