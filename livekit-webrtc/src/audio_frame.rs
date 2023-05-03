#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<i16>,
    pub sample_rate: u32,
    pub num_channels: u32,
    pub samples_per_channel: u32,
}

impl AudioFrame {
    pub fn new(sample_rate: u32, num_channels: u32, samples_per_channel: u32) -> Self {
        Self {
            data: vec![0; (num_channels * samples_per_channel) as usize],
            sample_rate,
            num_channels,
            samples_per_channel,
        }
    }
}
