use crate::proto;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit::webrtc::prelude::*;

impl From<proto::AudioSourceOptions> for AudioSourceOptions {
    fn from(opts: proto::AudioSourceOptions) -> Self {
        Self {
            echo_cancellation: opts.echo_cancellation,
            auto_gain_control: opts.auto_gain_control,
            noise_suppression: opts.noise_suppression,
        }
    }
}

impl proto::AudioFrameBufferInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, buffer: &AudioFrame) -> Self {
        Self {
            handle: Some(handle_id.into()),
            data_ptr: buffer.data.as_ptr() as u64,
            samples_per_channel: buffer.samples_per_channel,
            sample_rate: buffer.sample_rate,
            num_channels: buffer.num_channels,
        }
    }
}
