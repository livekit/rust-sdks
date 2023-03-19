#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub data: Vec<i16>,
    pub sample_rate_hz: u32,
    pub num_channels: usize,
    pub samples_per_channel: usize,
}
