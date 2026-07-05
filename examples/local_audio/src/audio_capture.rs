use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, SizedSample, Stream, StreamConfig};
use log::{error, info, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;

pub struct AudioCapture {
    _stream: Stream,
    is_running: Arc<AtomicBool>,
}

impl AudioCapture {
    pub async fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        db_tx: Option<mpsc::UnboundedSender<f32>>,
        channel_index: u32,      // New: Index of the channel to capture
        num_input_channels: u32, // New: Total number of channels in input
    ) -> Result<Self> {
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();

        let stream = match sample_format {
            SampleFormat::F32 => Self::create_input_stream::<f32>(
                device,
                config,
                audio_tx,
                db_tx,
                is_running_clone,
                channel_index,
                num_input_channels,
            )?,
            SampleFormat::I16 => Self::create_input_stream::<i16>(
                device,
                config,
                audio_tx,
                db_tx,
                is_running_clone,
                channel_index,
                num_input_channels,
            )?,
            SampleFormat::U16 => Self::create_input_stream::<u16>(
                device,
                config,
                audio_tx,
                db_tx,
                is_running_clone,
                channel_index,
                num_input_channels,
            )?,
            sample_format => {
                return Err(anyhow!("Unsupported sample format: {:?}", sample_format));
            }
        };

        stream.play()?;
        info!("Audio capture stream started");

        Ok(AudioCapture { _stream: stream, is_running })
    }

    fn create_input_stream<T>(
        device: Device,
        config: StreamConfig,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        db_tx: Option<mpsc::UnboundedSender<f32>>,
        is_running: Arc<AtomicBool>,
        channel_index: u32,      // New: Index of the channel to capture
        num_input_channels: u32, // New: Total number of channels in input
    ) -> Result<Stream>
    where
        T: SizedSample + Send + 'static,
    {
        let stream = device.build_input_stream(
            &config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !is_running.load(Ordering::Relaxed) {
                    return;
                }

                // Extract samples from the selected channel (assuming interleaved format)
                let converted: Vec<i16> = data
                    .iter()
                    .skip(channel_index as usize)
                    .step_by(num_input_channels as usize)
                    .map(|&sample| Self::convert_sample_to_i16(sample))
                    .collect();

                // Calculate and send dB level if channel is available (now on selected channel only)
                if let Some(ref db_sender) = db_tx {
                    let db_level = crate::db_meter::calculate_db_level(&converted);
                    if let Err(e) = db_sender.send(db_level) {
                        warn!("Failed to send dB level: {}", e);
                    }
                }

                if let Err(e) = audio_tx.send(converted) {
                    warn!("Failed to send audio data: {}", e);
                }
            },
            move |err| {
                error!("Audio input stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
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

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
