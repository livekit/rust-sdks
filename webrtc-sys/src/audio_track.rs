use crate::impl_thread_safety;
use std::sync::Arc;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    pub struct AudioSourceOptions {
        pub echo_cancellation: bool,
        pub noise_suppression: bool,
        pub auto_gain_control: bool,
    }

    extern "C++" {
        include!("livekit/media_stream_track.h");

        type MediaStreamTrack = crate::media_stream_track::ffi::MediaStreamTrack;
    }

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
        fn audio_options(self: &AudioTrackSource) -> AudioSourceOptions;
        fn set_audio_options(self: &AudioTrackSource, options: &AudioSourceOptions);
        fn new_audio_track_source(options: AudioSourceOptions) -> SharedPtr<AudioTrackSource>;

        fn audio_to_media(track: SharedPtr<AudioTrack>) -> SharedPtr<MediaStreamTrack>;
        unsafe fn media_to_audio(track: SharedPtr<MediaStreamTrack>) -> SharedPtr<AudioTrack>;
        fn _shared_audio_track() -> SharedPtr<AudioTrack>;
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
    observer: Arc<dyn AudioSink>,
}

impl AudioSinkWrapper {
    pub fn new(observer: Arc<dyn AudioSink>) -> Self {
        Self { observer }
    }

    fn on_data(&self, data: &[i16], sample_rate: i32, nb_channels: usize, nb_frames: usize) {
        self.observer
            .on_data(data, sample_rate, nb_channels, nb_frames);
    }
}
