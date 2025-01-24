use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::Stream;
use livekit::{
    webrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame},
    AudioFilterPlugin, AudioFilterSession,
};
use parking_lot::Mutex;

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
    Duration(AudioFilterAudioStream),
}

impl Stream for AudioStreamKind {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            AudioStreamKind::Native(native_stream) => Pin::new(native_stream).poll_next(cx),
            AudioStreamKind::Duration(duration_stream) => Pin::new(duration_stream).poll_next(cx),
        }
    }
}

pub struct AudioFilterAudioStream {
    inner: NativeAudioStream,
    session: AudioFilterSession,
    buffer: Vec<i16>,
    sample_rate: u32,
    num_channels: u32,
    frame_size: usize,
}

impl AudioFilterAudioStream {
    pub fn new(
        inner: NativeAudioStream,
        session: AudioFilterSession,
        duration: Duration,
        sample_rate: u32,
        num_channels: u32,
    ) -> Self {
        let frame_size =
            ((sample_rate as f64) * duration.as_secs_f64() * num_channels as f64) as usize;
        Self {
            inner,
            session,
            buffer: Vec::with_capacity(frame_size),
            sample_rate,
            num_channels,
            frame_size,
        }
    }
}

impl Stream for AudioFilterAudioStream {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.get_mut();

        while let Poll::Ready(Some(frame)) = Pin::new(&mut this.inner).poll_next(cx) {
            this.buffer.extend_from_slice(&frame.data);

            if this.buffer.len() >= this.frame_size {
                let data = this.buffer.drain(..this.frame_size).collect::<Vec<_>>();
                let mut out: Vec<i16> = Vec::with_capacity(this.frame_size);

                this.session.process_i16(this.frame_size, &data, &mut out);

                return Poll::Ready(Some(AudioFrame {
                    data: out.into(),
                    sample_rate: this.sample_rate,
                    num_channels: this.num_channels,
                    samples_per_channel: (this.frame_size / this.num_channels as usize) as u32,
                }));
            }
        }

        if this.buffer.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}
