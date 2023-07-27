use super::FfiHandle;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};
use livekit::webrtc::prelude::*;

pub struct FfiAudioSource {
    handle_id: FfiHandleId,
    source_type: proto::AudioSourceType,
    source: RtcAudioSource,
}

impl FfiHandle for FfiAudioSource {}

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

        let source = Self {
            handle_id: server.next_id(),
            source_type,
            source: source_inner,
        };

        let info = proto::AudioSourceInfo::from(
            proto::FfiOwnedHandle {
                id: source.handle_id,
            },
            &source,
        );
        server.store_handle(source.handle_id, source);
        Ok(info)
    }

    pub fn capture_frame(
        &self,
        server: &'static server::FfiServer,
        capture: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<()> {
        match self.source {
            #[cfg(not(target_arch = "wasm32"))]
            RtcAudioSource::Native(ref source) => {
                let frame = server.retrieve_handle::<AudioFrame>(capture.buffer_handle)?;
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
