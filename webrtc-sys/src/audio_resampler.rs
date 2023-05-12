use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/audio_resampler.h");

        type AudioResampler;

        unsafe fn remix_and_resample(
            self: Pin<&mut AudioResampler>,
            src: *const i16,
            samples_per_channel: usize,
            num_channels: usize,
            sample_rate: i32,
            dst_num_channels: usize,
            dst_sample_rate: i32,
        ) -> usize;

        unsafe fn data(self: &AudioResampler) -> *const i16;

        fn create_audio_resampler() -> UniquePtr<AudioResampler>;
    }
}

impl_thread_safety!(ffi::AudioResampler, Send + Sync);
