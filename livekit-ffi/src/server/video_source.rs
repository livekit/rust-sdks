// Copyright 2026 LiveKit, Inc.
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

use std::sync::Arc;

use livekit::webrtc::{
    prelude::*,
    video_frame::{FrameMetadata, VideoFrame},
};

use super::{colorcvt, FfiHandle};
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

pub struct FfiVideoSource {
    pub handle_id: FfiHandleId,
    pub source_type: proto::VideoSourceType,
    pub source: RtcVideoSource,
}

impl FfiHandle for FfiVideoSource {}

fn frame_metadata_from_proto(metadata: Option<proto::FrameMetadata>) -> Option<FrameMetadata> {
    let metadata = metadata?;
    let frame_metadata =
        FrameMetadata { user_timestamp: metadata.user_timestamp, frame_id: metadata.frame_id };

    (frame_metadata.user_timestamp.is_some() || frame_metadata.frame_id.is_some())
        .then_some(frame_metadata)
}

#[cfg(not(target_arch = "wasm32"))]
fn video_codec_from_proto(
    codec: proto::VideoCodec,
) -> livekit::webrtc::video_source::VideoCodec {
    use livekit::webrtc::video_source::VideoCodec;
    match codec {
        proto::VideoCodec::H264 => VideoCodec::H264,
        proto::VideoCodec::H265 => VideoCodec::H265,
        proto::VideoCodec::Vp8 => VideoCodec::Vp8,
        proto::VideoCodec::Vp9 => VideoCodec::Vp9,
        proto::VideoCodec::Av1 => VideoCodec::Av1,
    }
}

/// Forwards encoder-side feedback from the native source out to the FFI
/// client as `EncodedVideoSourceEvent`s.
#[cfg(not(target_arch = "wasm32"))]
struct EncodedObserverBridge {
    server: &'static server::FfiServer,
    source_handle: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl livekit::webrtc::video_source::native::EncodedVideoSourceObserver
    for EncodedObserverBridge
{
    fn on_keyframe_requested(&self) {
        let _ = self.server.send_event(proto::EncodedVideoSourceEvent {
            source_handle: self.source_handle,
            message: Some(proto::encoded_video_source_event::Message::KeyframeRequested(
                proto::encoded_video_source_event::KeyframeRequested {},
            )),
        }.into());
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        let _ = self.server.send_event(proto::EncodedVideoSourceEvent {
            source_handle: self.source_handle,
            message: Some(proto::encoded_video_source_event::Message::TargetBitrateChanged(
                proto::encoded_video_source_event::TargetBitrateChanged {
                    bitrate_bps,
                    framerate_fps,
                },
            )),
        }.into());
    }
}

impl FfiVideoSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewVideoSourceRequest,
    ) -> FfiResult<proto::OwnedVideoSource> {
        let source_type = new_source.r#type();
        let handle_id = server.next_id();
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
                use livekit::webrtc::video_source::{
                    native::NativeEncodedVideoSource, VideoResolution,
                };

                let options = new_source.encoded_options.as_ref().ok_or_else(|| {
                    FfiError::InvalidRequest(
                        "encoded_options is required for VIDEO_SOURCE_ENCODED".into(),
                    )
                })?;

                let codec = video_codec_from_proto(options.codec());
                let resolution = VideoResolution {
                    width: new_source.resolution.width,
                    height: new_source.resolution.height,
                };
                let source = NativeEncodedVideoSource::new(codec, resolution);

                source.set_observer(Arc::new(EncodedObserverBridge {
                    server,
                    source_handle: handle_id,
                }));

                RtcVideoSource::Encoded(source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported video source type".into())),
        };

        let video_source = Self { handle_id, source_type, source: source_inner };
        let source_info = proto::VideoSourceInfo::from(&video_source);
        server.store_handle(handle_id, video_source);

        Ok(proto::OwnedVideoSource {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: source_info,
        })
    }

    /// Returns the unique 16-bit id assigned to an encoded source by the
    /// WebRTC layer. `None` for non-encoded sources.
    pub fn encoded_source_id(&self) -> Option<u16> {
        #[cfg(not(target_arch = "wasm32"))]
        if let RtcVideoSource::Encoded(ref source) = self.source {
            return Some(source.source_id());
        }
        None
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
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Encoded(_) => {
                return Err(FfiError::InvalidRequest(
                    "capture_video_frame is not supported for encoded sources; \
                     use capture_encoded_video_frame instead"
                        .into(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn capture_encoded_frame(
        &self,
        _server: &'static server::FfiServer,
        capture: proto::CaptureEncodedVideoFrameRequest,
    ) -> FfiResult<proto::CaptureEncodedVideoFrameResponse> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcVideoSource::Encoded(ref source) => {
                use livekit::webrtc::video_source::EncodedFrameInfo;

                let info = EncodedFrameInfo {
                    is_keyframe: capture.is_keyframe,
                    has_sps_pps: capture.has_sps_pps.unwrap_or(false),
                    width: capture.width.unwrap_or(0),
                    height: capture.height.unwrap_or(0),
                    capture_time_us: capture.capture_time_us.unwrap_or(0),
                };
                let accepted = source.capture_frame(&capture.data, &info);
                Ok(proto::CaptureEncodedVideoFrameResponse { accepted })
            }
            _ => Err(FfiError::InvalidRequest(
                "capture_encoded_video_frame requires a VIDEO_SOURCE_ENCODED source".into(),
            )),
        }
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
        }))
        .unwrap();

        assert_eq!(metadata.user_timestamp, Some(123));
        assert_eq!(metadata.frame_id, Some(456));
    }
}
