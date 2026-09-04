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
use livekit::webrtc::{
    prelude::*,
    video_frame::{FrameMetadata, VideoFrame},
};

pub struct FfiVideoSource {
    pub handle_id: FfiHandleId,
    pub source_type: proto::VideoSourceType,
    pub source: RtcVideoSource,
}

impl FfiHandle for FfiVideoSource {}

fn frame_metadata_from_proto(metadata: Option<proto::FrameMetadata>) -> Option<FrameMetadata> {
    let metadata = metadata?;
    let frame_metadata = FrameMetadata {
        user_timestamp: metadata.user_timestamp,
        frame_id: metadata.frame_id,
        user_data: metadata.user_data,
    };

    (frame_metadata.user_timestamp.is_some()
        || frame_metadata.frame_id.is_some()
        || frame_metadata.user_data.is_some())
    .then_some(frame_metadata)
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

                let video_source = if new_source.encoded.unwrap_or(false) {
                    NativeVideoSource::new_encoded(new_source.resolution.into())
                } else {
                    let is_screencast = new_source.is_screencast.unwrap_or(false);
                    NativeVideoSource::new(new_source.resolution.into(), is_screencast)
                };
                RtcVideoSource::Native(video_source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video source type".into())),
        };

        let handle_id = server.next_id();

        #[cfg(not(target_arch = "wasm32"))]
        if let RtcVideoSource::Native(ref native_source) = source_inner {
            if new_source.encoded.unwrap_or(false) {
                let native_source = native_source.clone();
                let handle = server.async_runtime.spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
                    loop {
                        interval.tick().await;

                        let keyframe_requested = native_source.take_keyframe_request();
                        let rate_control = native_source.take_rate_control_request();

                        if keyframe_requested || rate_control.is_some() {
                            let event = proto::EncodedRateControlEvent {
                                source_handle: handle_id,
                                target_bitrate_bps: rate_control.map(|r| r.target_bitrate_bps),
                                framerate_fps: rate_control.map(|r| r.framerate_fps),
                                keyframe_requested,
                            };
                            let _ = server.send_event(
                                proto::VideoSourceEvent {
                                    source_handle: handle_id,
                                    message: Some(proto::video_source_event::Message::RateControl(
                                        event,
                                    )),
                                }
                                .into(),
                            );
                        }
                    }
                });
                server.watch_panic(handle);
            }
        }

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
                    frame_metadata: frame_metadata_from_proto(capture.metadata),
                    buffer,
                };

                source.capture_frame(&frame);
            }
            _ => {}
        }
        Ok(())
    }

    pub unsafe fn capture_encoded_frame(
        &self,
        _server: &'static server::FfiServer,
        capture: proto::CaptureEncodedVideoFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(ref source) => {
                use livekit::webrtc::video_frame::{
                    EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame,
                };
                use livekit::webrtc::video_source::VideoResolution;

                let codec = match capture.codec() {
                    proto::EncodedVideoCodec::EncodedCodecH264 => EncodedVideoCodec::H264,
                    proto::EncodedVideoCodec::EncodedCodecH265 => EncodedVideoCodec::H265,
                    proto::EncodedVideoCodec::EncodedCodecVp8 => EncodedVideoCodec::VP8,
                    proto::EncodedVideoCodec::EncodedCodecVp9 => EncodedVideoCodec::VP9,
                    proto::EncodedVideoCodec::EncodedCodecAv1 => EncodedVideoCodec::AV1,
                };
                let frame_type = match capture.frame_type() {
                    proto::EncodedFrameType::EncodedFrameKey => EncodedFrameType::Key,
                    proto::EncodedFrameType::EncodedFrameDelta => EncodedFrameType::Delta,
                };

                let frame = EncodedVideoFrame {
                    codec,
                    payload: &capture.data,
                    timestamp_us: capture.timestamp_us,
                    frame_type,
                    resolution: VideoResolution { width: capture.width, height: capture.height },
                    frame_metadata: frame_metadata_from_proto(capture.metadata),
                };

                source.capture_encoded_frame(&frame);
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::frame_metadata_from_proto;
    use crate::proto;

    #[test]
    fn empty_proto_frame_metadata_is_ignored() {
        assert!(frame_metadata_from_proto(Some(proto::FrameMetadata::default())).is_none());
    }

    #[test]
    fn proto_frame_metadata_preserves_present_fields() {
        let metadata = frame_metadata_from_proto(Some(proto::FrameMetadata {
            user_timestamp: Some(123),
            frame_id: Some(456),
            user_data: Some(vec![7, 8, 9]),
        }))
        .unwrap();

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, Some(456));
        assert_eq!(metadata.user_data, Some(vec![7, 8, 9]));
    }
}
