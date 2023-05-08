use crate::server::audio_frame::{FfiAudioSource, FfiAudioSream};
use crate::{proto, FfiHandleId};
use livekit::webrtc::prelude::*;

impl proto::AudioFrameBufferInfo {
    pub fn from(handle_id: FfiHandleId, buffer: &AudioFrame) -> Self {
        Self {
            handle: Some(handle_id.into()),
            data_ptr: buffer.data.as_ptr() as u64,
            samples_per_channel: buffer.samples_per_channel,
            sample_rate: buffer.sample_rate,
            num_channels: buffer.num_channels,
        }
    }
}

impl From<&FfiAudioSream> for proto::AudioStreamInfo {
    fn from(stream: &FfiAudioSream) -> Self {
        Self {
            handle: Some(proto::FfiHandleId {
                id: stream.handle_id() as u64,
            }),
            track_sid: stream.track_sid().clone().into(),
            r#type: stream.stream_type() as i32,
        }
    }
}

impl From<&FfiAudioSource> for proto::AudioSourceInfo {
    fn from(source: &FfiAudioSource) -> Self {
        Self {
            handle: Some(proto::FfiHandleId {
                id: source.handle_id() as u64,
            }),
            r#type: source.source_type() as i32,
        }
    }
}
