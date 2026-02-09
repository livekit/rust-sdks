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

use super::{colorcvt, FfiHandle};
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::{prelude::*, video_frame::VideoFrame};

pub struct FfiVideoSource {
    pub handle_id: FfiHandleId,
    pub source_type: proto::VideoSourceType,
    pub source: RtcVideoSource,
}

impl FfiHandle for FfiVideoSource {}

fn proto_codec_to_libwebrtc(codec: proto::VideoCodecType) -> VideoCodecType {
    match codec {
        proto::VideoCodecType::CodecVp8 => VideoCodecType::VP8,
        proto::VideoCodecType::CodecVp9 => VideoCodecType::VP9,
        proto::VideoCodecType::CodecAv1 => VideoCodecType::AV1,
        proto::VideoCodecType::CodecH264 => VideoCodecType::H264,
        proto::VideoCodecType::CodecH265 => VideoCodecType::H265,
    }
}

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

                let video_source = NativeVideoSource::new(new_source.resolution.into());
                RtcVideoSource::Native(video_source)
            }
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoSourceType::VideoSourceEncoded => {
                use livekit::webrtc::encoded_video_source::native::NativeEncodedVideoSource;

                let proto_codec = new_source
                    .codec
                    .and_then(|c| proto::VideoCodecType::try_from(c).ok())
                    .unwrap_or(proto::VideoCodecType::CodecH264);
                let codec = proto_codec_to_libwebrtc(proto_codec);
                let res = new_source.resolution;
                let video_source =
                    NativeEncodedVideoSource::new(res.width, res.height, codec);
                RtcVideoSource::Encoded(video_source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video source type".into())),
        };

        let handle_id = server.next_id();
        let video_source = Self { handle_id, source_type, source: source_inner };
        let source_info = proto::VideoSourceInfo::from(&video_source);
        server.store_handle(handle_id, video_source);

        Ok(proto::OwnedVideoSource {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: source_info,
        })
    }

    pub unsafe fn capture_frame(
        &self,
        _server: &'static server::FfiServer,
        capture: proto::CaptureVideoFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(ref source) => {
                let buffer = colorcvt::to_libwebrtc_buffer(capture.buffer.clone());
                let frame = VideoFrame {
                    rotation: capture.rotation().into(),
                    timestamp_us: capture.timestamp_us,
                    buffer,
                };

                source.capture_frame(&frame);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn capture_encoded_frame(
        &self,
        _server: &'static server::FfiServer,
        capture: proto::CaptureEncodedVideoFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Encoded(ref source) => {
                use livekit::webrtc::encoded_video_source::EncodedFrameInfo;

                let info = EncodedFrameInfo {
                    data: capture.data,
                    capture_time_us: capture.capture_time_us,
                    rtp_timestamp: capture.rtp_timestamp,
                    width: capture.width,
                    height: capture.height,
                    is_keyframe: capture.is_keyframe,
                    has_sps_pps: capture.has_sps_pps,
                    simulcast_index: capture.simulcast_index.unwrap_or(0),
                };
                source.capture_frame(&info);
            }
            _ => {
                return Err(FfiError::InvalidRequest(
                    "capture_encoded_frame called on non-encoded source".into(),
                ));
            }
        }
        Ok(())
    }
}
