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
    video_frame::{
        EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame, FrameMetadata, VideoFrame,
    },
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

fn encoded_video_codec_from_proto(codec: proto::VideoCodec) -> EncodedVideoCodec {
    match codec {
        proto::VideoCodec::H264 => EncodedVideoCodec::H264,
        proto::VideoCodec::H265 => EncodedVideoCodec::H265,
        proto::VideoCodec::Vp8 => EncodedVideoCodec::VP8,
        proto::VideoCodec::Vp9 => EncodedVideoCodec::VP9,
        proto::VideoCodec::Av1 => EncodedVideoCodec::AV1,
    }
}

fn encoded_frame_type_from_proto(frame_type: proto::EncodedFrameType) -> EncodedFrameType {
    match frame_type {
        proto::EncodedFrameType::EncodedFrameKey => EncodedFrameType::Key,
        proto::EncodedFrameType::EncodedFrameDelta => EncodedFrameType::Delta,
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

                let is_screencast = new_source.is_screencast.unwrap_or(false);
                let video_source =
                    NativeVideoSource::new(new_source.resolution.into(), is_screencast);
                RtcVideoSource::Native(video_source)
            }
            #[cfg(not(target_arch = "wasm32"))]
            proto::VideoSourceType::VideoSourceEncoded => {
                use livekit::webrtc::video_source::native::NativeVideoSource;

                let video_source = NativeVideoSource::new_encoded(new_source.resolution.into());
                RtcVideoSource::Native(video_source)
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
        if self.source_type != proto::VideoSourceType::VideoSourceNative {
            return Err(FfiError::InvalidRequest(
                "raw frames require a native video source".into(),
            ));
        }
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

    /// # Safety
    ///
    /// `capture.data_ptr` must address `capture.data_len` readable bytes for
    /// the duration of this call.
    pub unsafe fn capture_encoded_frame(
        &self,
        capture: proto::CaptureEncodedVideoFrameRequest,
    ) -> FfiResult<bool> {
        if self.source_type != proto::VideoSourceType::VideoSourceEncoded {
            return Err(FfiError::InvalidRequest(
                "encoded frames require an encoded video source".into(),
            ));
        }
        if capture.width == 0 || capture.height == 0 {
            return Err(FfiError::InvalidRequest(
                "encoded frame dimensions must be non-zero".into(),
            ));
        }
        let payload_len = usize::try_from(capture.data_len).map_err(|_| {
            FfiError::InvalidRequest("encoded frame payload length does not fit usize".into())
        })?;
        if capture.data_ptr == 0 {
            return Err(FfiError::InvalidRequest(
                "encoded frame payload pointer must be non-null".into(),
            ));
        }

        let codec = proto::VideoCodec::try_from(capture.codec)
            .map(encoded_video_codec_from_proto)
            .map_err(|_| FfiError::InvalidRequest("unknown encoded video codec".into()))?;
        let frame_type = proto::EncodedFrameType::try_from(capture.frame_type)
            .map(encoded_frame_type_from_proto)
            .map_err(|_| FfiError::InvalidRequest("unknown encoded frame type".into()))?;
        let payload = std::slice::from_raw_parts(capture.data_ptr as *const u8, payload_len);
        let frame = EncodedVideoFrame {
            codec,
            payload,
            timestamp_us: capture.timestamp_us,
            frame_type,
            resolution: VideoResolution { width: capture.width, height: capture.height },
            frame_metadata: frame_metadata_from_proto(capture.metadata),
        };

        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(ref source) => Ok(source.capture_encoded_frame(&frame)),
            _ => Err(FfiError::InvalidRequest(
                "encoded video sources are unavailable on this platform".into(),
            )),
        }
    }

    pub fn take_encoded_feedback(
        &self,
    ) -> FfiResult<proto::TakeEncodedVideoSourceFeedbackResponse> {
        if self.source_type != proto::VideoSourceType::VideoSourceEncoded {
            return Err(FfiError::InvalidRequest(
                "encoded feedback requires an encoded video source".into(),
            ));
        }

        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Native(ref source) => {
                let rate_control =
                    source.take_rate_control_request().map(|request| proto::EncodedRateControl {
                        target_bitrate_bps: request.target_bitrate_bps,
                        framerate_fps: request.framerate_fps,
                    });
                Ok(proto::TakeEncodedVideoSourceFeedbackResponse {
                    keyframe_requested: source.take_keyframe_request(),
                    rate_control,
                })
            }
            _ => Err(FfiError::InvalidRequest(
                "encoded video sources are unavailable on this platform".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        encoded_frame_type_from_proto, encoded_video_codec_from_proto, frame_metadata_from_proto,
    };
    use crate::proto;
    use livekit::webrtc::video_frame::{EncodedFrameType, EncodedVideoCodec};

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

    #[test]
    fn encoded_video_codecs_map_to_rust_types() {
        let cases = [
            (proto::VideoCodec::H264, EncodedVideoCodec::H264),
            (proto::VideoCodec::H265, EncodedVideoCodec::H265),
            (proto::VideoCodec::Vp8, EncodedVideoCodec::VP8),
            (proto::VideoCodec::Vp9, EncodedVideoCodec::VP9),
            (proto::VideoCodec::Av1, EncodedVideoCodec::AV1),
        ];
        for (proto_codec, expected) in cases {
            assert_eq!(encoded_video_codec_from_proto(proto_codec), expected);
        }
    }

    #[test]
    fn encoded_frame_types_map_to_rust_types() {
        assert_eq!(
            encoded_frame_type_from_proto(proto::EncodedFrameType::EncodedFrameKey),
            EncodedFrameType::Key
        );
        assert_eq!(
            encoded_frame_type_from_proto(proto::EncodedFrameType::EncodedFrameDelta),
            EncodedFrameType::Delta
        );
    }
}
