use crate::{audio_frame::AudioFrame, media_stream::RtcAudioTrack};
use cxx::UniquePtr;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use webrtc_sys::media_stream as sys_ms;

pub struct NativeAudioStream {
    native_observer: UniquePtr<sys_ms::ffi::NativeAudioSink>,
    _observer: Box<AudioTrackObserver>,
    audio_track: RtcAudioTrack,
    frame_rx: mpsc::UnboundedReceiver<AudioFrame>,
}

impl NativeAudioStream {
    pub fn new(audio_track: RtcAudioTrack) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let mut observer = Box::new(AudioTrackObserver { frame_tx });
        let mut native_observer = unsafe {
            sys_ms::ffi::new_native_audio_sink(Box::new(sys_ms::AudioSinkWrapper::new(
                &mut *observer,
            )))
        };

        unsafe {
            sys_ms::ffi::media_to_audio(audio_track.sys_handle())
                .add_sink(native_observer.pin_mut());
        }

        Self {
            native_observer,
            _observer: observer,
            audio_track,
            frame_rx,
        }
    }

    pub fn track(&self) -> RtcAudioTrack {
        self.audio_track.clone()
    }

    pub fn close(&mut self) {
        self.frame_rx.close();
        unsafe {
            sys_ms::ffi::media_to_audio(self.audio_track.sys_handle())
                .remove_sink(self.native_observer.pin_mut());
        }
    }
}

impl Drop for NativeAudioStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeAudioStream {
    type Item = AudioFrame;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_rx.poll_recv(cx)
    }
}

pub struct AudioTrackObserver {
    frame_tx: mpsc::UnboundedSender<AudioFrame>,
}

impl sys_ms::AudioSink for AudioTrackObserver {
    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
        // TODO(theomonnom): Should we avoid copy here?
        let _ = self.frame_tx.send(AudioFrame {
            data: data.to_owned(),
            sample_rate: sample_rate as u32,
            num_channels: nb_channels as u32,
            samples_per_channel: nb_frames as u32,
        });
    }
}
