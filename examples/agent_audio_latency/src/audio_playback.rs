use crate::audio_mixer::AudioMixer;
use crate::latency::TurnLatencyBench;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use log::{error, info};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub struct AudioPlayback {
    _stream: Stream,
    is_running: Arc<AtomicBool>,
}

impl AudioPlayback {
    pub fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        mixer: AudioMixer,
        benchmark: Option<Arc<Mutex<TurnLatencyBench>>>,
    ) -> Result<Self> {
        let is_running = Arc::new(AtomicBool::new(true));
        let stream = match sample_format {
            SampleFormat::F32 => Self::create_output_stream::<f32>(
                device,
                config,
                mixer,
                is_running.clone(),
                benchmark.clone(),
            )?,
            SampleFormat::I16 => Self::create_output_stream::<i16>(
                device,
                config,
                mixer,
                is_running.clone(),
                benchmark.clone(),
            )?,
            SampleFormat::U16 => Self::create_output_stream::<u16>(
                device,
                config,
                mixer,
                is_running.clone(),
                benchmark,
            )?,
            other => return Err(anyhow!("unsupported output sample format: {other:?}")),
        };

        stream.play()?;
        info!("audio playback stream started");

        Ok(Self { _stream: stream, is_running })
    }

    fn create_output_stream<T>(
        device: Device,
        config: StreamConfig,
        mixer: AudioMixer,
        is_running: Arc<AtomicBool>,
        benchmark: Option<Arc<Mutex<TurnLatencyBench>>>,
    ) -> Result<Stream>
    where
        T: SizedSample + Sample + Send + 'static + FromSample<f32>,
    {
        // Speaker rendering also runs on a separate real-time audio thread managed by cpal.
        // It must not be blocked by network or room-event work, which is why playback pulls
        // already-mixed samples from shared state instead of awaiting Tokio work here.
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !is_running.load(Ordering::Relaxed) {
                    for sample in data.iter_mut() {
                        *sample = Sample::from_sample(0.0f32);
                    }
                    return;
                }

                let samples = mixer.get_samples(data.len());
                if let Some(benchmark) = &benchmark {
                    // Detect speaker response on the render callback thread so the benchmark
                    // measures when audio is actually handed to the output device path.
                    benchmark.lock().unwrap().observe_speaker_audio(&samples, config.sample_rate.0);
                }
                for (slot, sample) in data.iter_mut().zip(samples.into_iter()) {
                    *slot = convert_i16_to_sample::<T>(sample);
                }
            },
            move |err| {
                error!("audio output stream error: {err}");
            },
            None,
        )?;

        Ok(stream)
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        self.stop();
    }
}

fn convert_i16_to_sample<T: SizedSample + Sample + FromSample<f32>>(sample: i16) -> T {
    if std::mem::size_of::<T>() == std::mem::size_of::<f32>() {
        let sample_f32 = sample as f32 / i16::MAX as f32;
        unsafe { std::mem::transmute_copy::<f32, T>(&sample_f32) }
    } else if std::mem::size_of::<T>() == std::mem::size_of::<i16>() {
        unsafe { std::mem::transmute_copy::<i16, T>(&sample) }
    } else if std::mem::size_of::<T>() == std::mem::size_of::<u16>() {
        let sample_u16 = ((sample as i32) + (u16::MAX as i32 / 2)) as u16;
        unsafe { std::mem::transmute_copy::<u16, T>(&sample_u16) }
    } else {
        Sample::from_sample(0.0f32)
    }
}
