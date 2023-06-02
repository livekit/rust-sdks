use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/audio_track.h");

        type AudioTrack;
        type NativeAudioSink;
        type AudioTrackSource;

        fn add_sink(self: &AudioTrack, sink: &SharedPtr<NativeAudioSink>);
        fn remove_sink(self: &AudioTrack, sink: &SharedPtr<NativeAudioSink>);
        fn new_native_audio_sink(observer: Box<AudioSinkWrapper>) -> SharedPtr<NativeAudioSink>;

        fn on_captured_frame(
            self: &AudioTrackSource,
            data: &[i16],
            sample_rate: i32,
            nb_channels: usize,
            nb_frames: usize,
        );
        fn new_audio_track_source() -> SharedPtr<AudioTrackSource>;
    }

    extern "Rust" {
        type AudioSinkWrapper;

        fn on_data(
            self: &AudioSinkWrapper,
            data: &[i16],
            sample_rate: i32,
            nb_channels: usize,
            nb_frames: usize,
        );
    }
}

impl_thread_safety!(ffi::AudioTrack, Send + Sync);
impl_thread_safety!(ffi::NativeAudioSink, Send + Sync);
impl_thread_safety!(ffi::AudioTrackSource, Send + Sync);

pub trait AudioSink: Send {
    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize);
}

pub struct AudioSinkWrapper {
    observer: Box<dyn AudioSink>,
}

impl AudioSinkWrapper {
    pub fn new(observer: Box<dyn AudioSink>) -> Self {
        Self { observer }
    }

    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
        self.observer
            .on_data(data, sample_rate, nb_channels, nb_frames);
    }
}
