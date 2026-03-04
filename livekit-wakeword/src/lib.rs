// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::Path;
use std::sync::Once;

use ort::session::Session;

static INIT_TRACT: Once = Once::new();

pub(crate) fn ensure_tract_backend() {
    INIT_TRACT.call_once(|| {
        ort::set_api(ort_tract::api());
    });
}

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
    ensure_tract_backend();
    Ok(Session::builder()?.commit_from_memory(bytes)?)
}

pub(crate) fn build_session_from_file(
    path: impl AsRef<Path>,
) -> Result<Session, Box<dyn std::error::Error>> {
    ensure_tract_backend();
    let bytes = std::fs::read(path)?;
    Ok(Session::builder()?.commit_from_memory(&bytes)?)
}

#[cfg(test)]
fn generate_sine(freq: f64, sample_rate: usize, duration: f64) -> Vec<i16> {
    (0..((sample_rate as f64 * duration) as usize))
        .map(|i| {
            let t = (i as f64) / (sample_rate as f64);
            (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
        })
        .collect()
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
