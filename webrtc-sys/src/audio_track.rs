// Copyright 2023 LiveKit, Inc.
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

use crate::impl_thread_safety;

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
            sample_rate: u32,
            nb_channels: u32,
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
        self.observer.on_data(data, sample_rate, nb_channels, nb_frames);
    }
}
