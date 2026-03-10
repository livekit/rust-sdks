use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AudioMixer {
    buffer: Arc<Mutex<VecDeque<i16>>>,
    volume: f32,
    max_buffer_size: usize,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u32, volume: f32) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::with_capacity(
                sample_rate as usize * channels as usize,
            ))),
            volume: volume.clamp(0.0, 1.0),
            max_buffer_size: sample_rate as usize * channels as usize,
        }
    }

    pub fn add_audio_data(&self, data: &[i16]) {
        let mut buffer = self.buffer.lock().unwrap();
        for &sample in data {
            buffer.push_back((sample as f32 * self.volume) as i16);
            if buffer.len() > self.max_buffer_size {
                buffer.pop_front();
            }
        }
    }

    pub fn get_samples(&self, requested_samples: usize) -> Vec<i16> {
        let mut buffer = self.buffer.lock().unwrap();
        let mut result = Vec::with_capacity(requested_samples);

        for _ in 0..requested_samples {
            result.push(buffer.pop_front().unwrap_or(0));
        }

        result
    }
}
