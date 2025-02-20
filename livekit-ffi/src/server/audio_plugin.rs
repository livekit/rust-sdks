use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::Stream;
use livekit::{
    webrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame},
    AudioFilterAudioStream, AudioFilterPlugin,
};

use super::FfiHandle;
use crate::FfiHandleId;

#[derive(Clone)]
pub struct FfiAudioFilterPlugin {
    pub handle_id: FfiHandleId,
    pub plugin: Arc<AudioFilterPlugin>,
}

impl FfiHandle for FfiAudioFilterPlugin {}

pub trait AudioStream: Stream<Item = AudioFrame<'static>> + Send + Sync + Unpin {
    fn close(&mut self);
}

pub enum AudioStreamKind {
    Native(NativeAudioStream),
    Filtered(AudioFilterAudioStream),
}

impl Stream for AudioStreamKind {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            AudioStreamKind::Native(native_stream) => Pin::new(native_stream).poll_next(cx),
            AudioStreamKind::Filtered(duration_stream) => Pin::new(duration_stream).poll_next(cx),
        }
    }
}
