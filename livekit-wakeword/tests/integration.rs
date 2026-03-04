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

use std::path::PathBuf;

use livekit_wakeword::{WakeWordModel, SAMPLE_RATE};

fn classifier_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("onnx/hey_livekit.onnx")
}

fn generate_sine(freq: f64, sample_rate: usize, duration: f64) -> Vec<i16> {
    (0..((sample_rate as f64 * duration) as usize))
        .map(|i| {
            let t = (i as f64) / (sample_rate as f64);
            (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
        })
        .collect()
}

#[test]
fn test_predict() {
    let mut model = WakeWordModel::new(&[classifier_path()], SAMPLE_RATE as u32).unwrap();

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
