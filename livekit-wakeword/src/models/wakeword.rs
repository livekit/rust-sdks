struct WakeWordModel {}

struct Detection {
    name: String,
    timestamp: u64,
    confidence: f32,
}

pub trait Detector {
    fn detect(&self, audio: &[f32]) -> bool;
}
