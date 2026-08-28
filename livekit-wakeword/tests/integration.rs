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

use livekit_wakeword::{OptimizationLevel, SessionOptions, WakeWordModel, SAMPLE_RATE};

mod common;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

fn classifier_path() -> PathBuf {
    fixtures_dir().join("hey_livekit.onnx")
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

const THRESHOLD: f32 = 0.5;

/// Test that a positive WAV sample (containing the wake word) scores above the threshold.
#[test]
fn test_positive_wav_above_threshold() {
    let (sample_rate, samples) = common::read_wav("positive.wav");
    let mut model = WakeWordModel::new(&[classifier_path()], sample_rate).unwrap();

    let predictions = model.predict(&samples).unwrap();
    let score = predictions["hey_livekit"];
    println!("positive.wav score: {score}");
    assert!(
        score >= THRESHOLD,
        "expected positive sample score ({score}) >= threshold ({THRESHOLD})"
    );
}

/// Test that a negative WAV sample (NOT containing the wake word) scores below the threshold.
#[test]
fn test_negative_wav_below_threshold() {
    let (sample_rate, samples) = common::read_wav("negative.wav");
    let mut model = WakeWordModel::new(&[classifier_path()], sample_rate).unwrap();

    let predictions = model.predict(&samples).unwrap();
    let score = predictions["hey_livekit"];
    println!("negative.wav score: {score}");
    assert!(
        score < THRESHOLD,
        "expected negative sample score ({score}) < threshold ({THRESHOLD})"
    );
}

/// Session options supplied at runtime must not change what the model predicts,
/// including the options a backend cannot honour (`ort-tract` implements only the
/// graph optimization level and skips the rest).
#[test]
fn test_session_options_preserve_scores() {
    let (sample_rate, samples) = common::read_wav("positive.wav");

    let options = SessionOptions {
        intra_threads: Some(1),
        inter_threads: Some(1),
        parallel_execution: Some(false),
        intra_op_spinning: Some(false),
        inter_op_spinning: Some(false),
        config_entries: vec![("session.use_env_allocators".to_string(), "1".to_string())],
        ..Default::default()
    };

    let mut default = WakeWordModel::new(&[classifier_path()], sample_rate).unwrap();
    let mut tuned =
        WakeWordModel::with_session_options(&[classifier_path()], sample_rate, options).unwrap();

    assert_eq!(
        default.predict(&samples).unwrap()["hey_livekit"],
        tuned.predict(&samples).unwrap()["hey_livekit"]
    );
}

/// Every optimization level builds a working model, and a per-classifier override
/// leaves the model's own options in place.
#[test]
fn test_optimization_levels_and_per_model_options() {
    let (sample_rate, samples) = common::read_wav("positive.wav");

    for level in [OptimizationLevel::Disable, OptimizationLevel::Level1, OptimizationLevel::Level3]
    {
        let options = SessionOptions { optimization_level: level, ..Default::default() };
        let mut model =
            WakeWordModel::with_session_options(&[classifier_path()], sample_rate, options)
                .unwrap();
        let score = model.predict(&samples).unwrap()["hey_livekit"];
        assert!(score > THRESHOLD, "{level:?} scored {score}");
    }

    let no_models: &[PathBuf] = &[];
    let mut model = WakeWordModel::new(no_models, sample_rate).unwrap();
    let options =
        SessionOptions { optimization_level: OptimizationLevel::Disable, ..Default::default() };
    model
        .load_model_with_session_options(classifier_path(), Some("unoptimized"), &options)
        .unwrap();
    model.load_model(classifier_path(), Some("default")).unwrap();

    let predictions = model.predict(&samples).unwrap();
    assert_eq!(predictions["unoptimized"], predictions["default"]);
}
