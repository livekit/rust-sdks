use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AudioMixer {
    buffer: Arc<Mutex<std::collections::VecDeque<i16>>>,
    sample_rate: u32,
    channels: u32,
    volume: f32,
    max_buffer_size: usize,
}

impl AudioMixer {
    pub fn new(sample_rate: u32, channels: u32, volume: f32) -> Self {
        // Buffer for 1 second of audio
        let max_buffer_size = sample_rate as usize * channels as usize;
        
        Self {
            buffer: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(max_buffer_size))),
            sample_rate,
            channels,
            volume: volume.clamp(0.0, 1.0),
            max_buffer_size,
        }
    }

    pub fn add_audio_data(&self, data: &[i16]) {
        let mut buffer = self.buffer.lock().unwrap();
        
        // Apply volume scaling and add to buffer
        for &sample in data.iter() {
            let scaled_sample = (sample as f32 * self.volume) as i16;
            buffer.push_back(scaled_sample);
            
            // Prevent buffer from growing too large
            if buffer.len() > self.max_buffer_size {
                buffer.pop_front();
            }
        }
    }

    pub fn get_samples(&self, requested_samples: usize) -> Vec<i16> {
        let mut buffer = self.buffer.lock().unwrap();
        let mut result = Vec::with_capacity(requested_samples);
        
        // Fill the requested samples
        for _ in 0..requested_samples {
            if let Some(sample) = buffer.pop_front() {
                result.push(sample);
            } else {
                result.push(0); // Silence when no data available
            }
        }
        
        result
    }
    
    pub fn buffer_size(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }
} 