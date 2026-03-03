use std::path::Path;

use ort::session::{builder::GraphOptimizationLevel, Session};

pub mod embedding;
pub mod melspectrogram;
pub mod wakeword;

pub use wakeword::WakeWordModel;

pub const SAMPLE_RATE: usize = 16000;
pub const MEL_BINS: usize = 32;
pub const EMBEDDING_WINDOW: usize = 76; // mel frames per embedding
pub const EMBEDDING_STRIDE: usize = 8; // mel frames between embeddings
pub const EMBEDDING_DIM: usize = 96;
pub const MIN_EMBEDDINGS: usize = 16; // classifier input length

pub(crate) fn build_session_from_memory(
    bytes: &[u8],
) -> Result<Session, Box<dyn std::error::Error>> {
    Ok(Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_memory(bytes)?)
}

pub(crate) fn build_session_from_file(
    path: impl AsRef<Path>,
) -> Result<Session, Box<dyn std::error::Error>> {
    Ok(Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(path)?)
}

#[cfg(test)]
fn generate_sine(freq: f64, sample_rate: usize, duration: f64) -> ndarray::Array1<i16> {
    ndarray::Array1::from_iter((0..((sample_rate as f64 * duration) as usize)).map(|i| {
        let t = (i as f64) / (sample_rate as f64);
        (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn classifier_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("onnx/hey_livekit.onnx")
    }

    #[test]
    fn test_predict() {
        let mut model = WakeWordModel::new(&[classifier_path()]).unwrap();

        // Full pipeline: audio -> mel -> embeddings -> classifier score
        let audio = generate_sine(440.0, SAMPLE_RATE, 2.0);
        let predictions = model.predict(&audio).unwrap();
        assert!(predictions.contains_key("hey_livekit"));
        assert!((0.0..=1.0).contains(&predictions["hey_livekit"]));

        // Too-short audio returns zero
        let short = generate_sine(440.0, SAMPLE_RATE, 0.1);
        let predictions = model.predict(&short).unwrap();
        assert_eq!(predictions["hey_livekit"], 0.0);
    }
}
