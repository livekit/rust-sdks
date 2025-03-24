use std::{pin::Pin, sync::Arc};

use ffi::AudioFrameInfo;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/audio_mixer.h");

        type AudioMixer;

        unsafe fn add_source(self: Pin<&mut AudioMixer>, src: Box<AudioMixerSourceWrapper>);

        unsafe fn remove_source(self: Pin<&mut AudioMixer>, ssrc: i32);

        unsafe fn mix(self: Pin<&mut AudioMixer>, num_channels: usize) -> usize;

        unsafe fn data(self: &AudioMixer) -> *const i16;

        fn create_audio_mixer() -> UniquePtr<AudioMixer>;

        type NativeAudioFrame;

        unsafe fn update_frame(
            self: Pin<&mut NativeAudioFrame>,
            timestamp: u32,
            data: *const i16,
            samples_per_channel: usize,
            sample_rate_hz: i32,
            num_channels: usize,
        );
    }

    pub enum AudioFrameInfo {
        Normal,
        Muted,
        Error,
    }

    extern "Rust" {
        type AudioMixerSourceWrapper;

        fn ssrc(self: &AudioMixerSourceWrapper) -> i32;
        fn preferred_sample_rate(self: &AudioMixerSourceWrapper) -> i32;
        fn get_audio_frame_with_info(
            self: &AudioMixerSourceWrapper,
            target_sample_rate: i32,
            frame: Pin<&mut NativeAudioFrame>,
        ) -> AudioFrameInfo;
    }
}

pub trait AudioMixerSource {
    fn ssrc(&self) -> i32;
    fn preferred_sample_rate(&self) -> i32;
    fn get_audio_frame_with_info<'a>(
        &self,
        target_sample_rate: i32,
        frame: NativeAudioFrame<'a>,
    ) -> AudioFrameInfo;
}

pub struct AudioMixerSourceWrapper {
    source: Arc<dyn AudioMixerSource>,
}

pub type NativeAudioFrame<'a> = Pin<&'a mut ffi::NativeAudioFrame>;

impl AudioMixerSourceWrapper {
    pub fn new(source: Arc<dyn AudioMixerSource>) -> Self {
        Self { source }
    }

    pub fn ssrc(&self) -> i32 {
        self.source.ssrc()
    }
    pub fn preferred_sample_rate(&self) -> i32 {
        self.source.preferred_sample_rate()
    }

    pub fn get_audio_frame_with_info(
        &self,
        target_sample_rate: i32,
        frame: Pin<&mut ffi::NativeAudioFrame>,
    ) -> AudioFrameInfo {
        self.source.get_audio_frame_with_info(target_sample_rate, frame)
    }
}

impl_thread_safety!(ffi::AudioMixer, Send + Sync);
