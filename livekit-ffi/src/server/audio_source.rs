use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::*;
use log::warn;
use tokio::sync::oneshot;

pub struct FfiAudioSource {
    handle_id: FfiHandleId,
    source_type: proto::AudioSourceType,
    source: RtcAudioSource,
}

impl FfiAudioSource {
    pub fn setup(
        server: &'static server::FfiServer,
        new_source: proto::NewAudioSourceRequest,
    ) -> FfiResult<proto::AudioSourceInfo> {
        let source_type = proto::AudioSourceType::from_i32(new_source.r#type).unwrap();
        #[allow(unreachable_patterns)]
        let source_inner = match source_type {
            #[cfg(not(target_arch = "wasm32"))]
            proto::AudioSourceType::AudioSourceNative => {
                use livekit::webrtc::audio_source::native::NativeAudioSource;
                let audio_source =
                    NativeAudioSource::new(new_source.options.map(Into::into).unwrap_or_default());
                RtcAudioSource::Native(audio_source)
            }
            _ => return Err(FfiError::InvalidRequest("unsupported audio source type")),
        };

        let audio_source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };
        let source_info = proto::AudioSourceInfo::from(&audio_source);

        server
            .ffi_handles
            .insert(audio_source.handle_id, Box::new(audio_source));

        Ok(source_info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => {
                let frame = server
                    .ffi_handles
                    .get(&capture.buffer_handle)
                    .ok_or(FfiError::InvalidRequest("handle not found"))?;

                let frame = frame
                    .downcast_ref::<AudioFrame>()
                    .ok_or(FfiError::InvalidRequest("handle is not an audio frame"))?;

                source.capture_frame(frame);
            }
            _ => {}
        }

        Ok(())
    }

    pub fn handle_id(&self) -> FfiHandleId {
        self.handle_id
    }

    pub fn source_type(&self) -> proto::AudioSourceType {
        self.source_type
    }

    pub fn inner_source(&self) -> &RtcAudioSource {
        &self.source
    }
}
