use crate::latency::TurnLatencyBench;
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, SizedSample, Stream, StreamConfig};
use log::{error, info, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc;

pub struct AudioCapture {
    _stream: Stream,
    is_running: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        channel_index: u32,
        num_input_channels: u32,
        benchmark: Option<Arc<Mutex<TurnLatencyBench>>>,
    ) -> Result<Self> {
        let is_running = Arc::new(AtomicBool::new(true));
        let stream = match sample_format {
            SampleFormat::F32 => Self::create_input_stream::<f32>(
                device,
                config,
                audio_tx,
                is_running.clone(),
                channel_index,
                num_input_channels,
                benchmark.clone(),
            )?,
            SampleFormat::I16 => Self::create_input_stream::<i16>(
                device,
                config,
                audio_tx,
                is_running.clone(),
                channel_index,
                num_input_channels,
                benchmark.clone(),
            )?,
            SampleFormat::U16 => Self::create_input_stream::<u16>(
                device,
                config,
                audio_tx,
                is_running.clone(),
                channel_index,
                num_input_channels,
                benchmark,
            )?,
            other => return Err(anyhow!("unsupported input sample format: {other:?}")),
        };

        stream.play()?;
        info!("audio capture stream started");

        Ok(Self { _stream: stream, is_running })
    }

    fn create_input_stream<T>(
        device: Device,
        config: StreamConfig,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        is_running: Arc<AtomicBool>,
        channel_index: u32,
        num_input_channels: u32,
        benchmark: Option<Arc<Mutex<TurnLatencyBench>>>,
    ) -> Result<Stream>
    where
        T: SizedSample + Send + 'static,
    {
        // cpal runs the microphone callback on the platform's real-time audio thread.
        // Keep this callback short and non-blocking: push samples into a channel and let
        // the dedicated uplink runtime handle framing and SDK calls.
        let stream = device.build_input_stream(
            &config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !is_running.load(Ordering::Relaxed) {
                    return;
                }

                let converted: Vec<i16> = data
                    .iter()
                    .skip(channel_index as usize)
                    .step_by(num_input_channels as usize)
                    .map(|&sample| convert_sample_to_i16(sample))
                    .collect();

                if let Some(benchmark) = &benchmark {
                    // Detect turn-end directly on the capture callback thread. For this
                    // benchmark, the audio callback timing is more meaningful than a later
                    // async task wakeup in the networking pipeline.
                    benchmark.lock().unwrap().observe_user_audio(&converted, config.sample_rate.0);
                }

                if let Err(err) = audio_tx.send(converted) {
                    warn!("failed to forward captured audio: {err}");
                }
            },
            move |err| {
                error!("audio input stream error: {err}");
            },
            None,
        )?;

        Ok(stream)
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn convert_sample_to_i16<T: SizedSample>(sample: T) -> i16 {
    if std::mem::size_of::<T>() == std::mem::size_of::<f32>() {
        let sample_f32 = unsafe { std::mem::transmute_copy::<T, f32>(&sample) };
        (sample_f32.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    } else if std::mem::size_of::<T>() == std::mem::size_of::<i16>() {
        unsafe { std::mem::transmute_copy::<T, i16>(&sample) }
    } else if std::mem::size_of::<T>() == std::mem::size_of::<u16>() {
        let sample_u16 = unsafe { std::mem::transmute_copy::<T, u16>(&sample) };
        ((sample_u16 as i32) - (u16::MAX as i32 / 2)) as i16
    } else {
        0
    }
}
