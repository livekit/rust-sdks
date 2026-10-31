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

use livekit_wakeword::{
    GraphOptimizationLevel, SessionOptions, WakeWordError, WakeWordModel, FRAME_SAMPLES,
    SAMPLE_RATE,
};

mod common;

const THRESHOLD: f32 = 0.5;
const UNEVEN_CHUNKS: [usize; 5] = [700, 1860, 1280, 333, 2227];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name)
}

fn hey_livekit() -> WakeWordModel {
    WakeWordModel::new(&[fixture("hey_livekit.onnx")], SAMPLE_RATE as u32).unwrap()
}

fn tiny() -> WakeWordModel {
    WakeWordModel::new(&[fixture("streaming_tiny.onnx")], SAMPLE_RATE as u32).unwrap()
}

/// The deterministic chirp `streaming_tiny_expected.json` was scored on by the
/// Python runtime (shared with the Swift tests).
fn chirp() -> Vec<i16> {
    (0..SAMPLE_RATE * 3)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE as f64;
            (f64::sin(2.0 * std::f64::consts::PI * (150.0 + 400.0 * t) * t) * 9000.0) as i16
        })
        .collect()
}

/// Per-80 ms scores the Python runtime produced for [`chirp`].
fn python_scores() -> Vec<f32> {
    let json = std::fs::read_to_string(fixture("streaming_tiny_expected.json")).unwrap();
    let start = json.find('[').unwrap() + 1;
    let end = json[start..].find(']').unwrap() + start;
    json[start..end].split(',').map(|s| s.trim().parse().unwrap()).collect()
}

fn generate_sine(freq: f64, duration: f64) -> Vec<i16> {
    (0..((SAMPLE_RATE as f64 * duration) as usize))
        .map(|i| {
            let t = (i as f64) / (SAMPLE_RATE as f64);
            (f64::sin(2.0 * std::f64::consts::PI * freq * t) * 32767.0) as i16
        })
        .collect()
}

/// The Rust runtime reproduces the Python one step for step.
#[test]
fn test_matches_python_runtime() {
    let mut model = tiny();
    let audio = chirp();
    let expected = python_scores();

    let scores: Vec<f32> = audio
        .chunks_exact(FRAME_SAMPLES)
        .map(|frame| model.predict(frame).unwrap()["streaming_tiny"])
        .collect();
    assert_eq!(scores.len(), expected.len());
    for (step, (a, b)) in scores.iter().zip(&expected).enumerate() {
        assert!((a - b).abs() < 1e-4, "step {step}: rust {a} vs python {b}");
    }
}

/// Uneven chunks give the same stream as 80 ms frames, and `reset` restarts it.
#[test]
fn test_chunking_and_reset() {
    let mut model = tiny();
    let audio = chirp();
    let expected_max = python_scores().into_iter().fold(0.0, f32::max);

    let mut best = 0.0f32;
    let mut pos = 0;
    for size in UNEVEN_CHUNKS.iter().cycle() {
        if pos >= audio.len() {
            break;
        }
        let end = audio.len().min(pos + size);
        best = best.max(model.predict(&audio[pos..end]).unwrap()["streaming_tiny"]);
        pos = end;
    }
    assert!((best - expected_max).abs() < 1e-4, "{best} vs {expected_max}");

    model.reset().unwrap();
    let first = model.predict(&audio[..FRAME_SAMPLES]).unwrap()["streaming_tiny"];
    assert!((first - python_scores()[0]).abs() < 1e-4);
}

/// The model remembers what it heard: the same audio scores differently the
/// second time, until a reset.
#[test]
fn test_state_is_carried() {
    let mut model = tiny();
    let audio = &chirp()[..FRAME_SAMPLES * 20];
    let run = |model: &mut WakeWordModel| -> Vec<f32> {
        audio
            .chunks_exact(FRAME_SAMPLES)
            .map(|f| model.predict(f).unwrap()["streaming_tiny"])
            .collect()
    };

    let first = run(&mut model);
    assert_ne!(first, run(&mut model));
    model.reset().unwrap();
    assert_eq!(first, run(&mut model));
}

#[test]
fn test_scores_are_probabilities() {
    let mut model = hey_livekit();
    let sine = generate_sine(440.0, 2.0);
    let score = model.predict_clip(&sine).unwrap()["hey_livekit"];
    assert!((0.0..=1.0).contains(&score));

    // Less than one 80 ms step completes nothing.
    assert_eq!(model.predict_clip(&sine[..800]).unwrap()["hey_livekit"], 0.0);
}

/// Test that a positive WAV sample (containing the wake word) scores above the threshold.
#[test]
fn test_positive_wav_above_threshold() {
    let (sample_rate, samples) = common::read_wav("positive.wav");
    let mut model = WakeWordModel::new(&[fixture("hey_livekit.onnx")], sample_rate).unwrap();

    let score = model.predict_clip(&samples).unwrap()["hey_livekit"];
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
    let mut model = WakeWordModel::new(&[fixture("hey_livekit.onnx")], sample_rate).unwrap();

    let score = model.predict_clip(&samples).unwrap()["hey_livekit"];
    println!("negative.wav score: {score}");
    assert!(
        score < THRESHOLD,
        "expected negative sample score ({score}) < threshold ({THRESHOLD})"
    );
}

/// Audio at another rate is resampled to 16 kHz before it reaches the model.
#[test]
fn test_resampled_input() {
    let (_, samples) = common::read_wav("positive.wav");
    // Linear interpolation to 48 kHz.
    let upsampled: Vec<i16> = samples
        .windows(2)
        .flat_map(|w| {
            let (a, b) = (w[0] as f32, w[1] as f32);
            [a, a + (b - a) / 3.0, a + 2.0 * (b - a) / 3.0].map(|x| x as i16)
        })
        .collect();

    let mut model = WakeWordModel::new(&[fixture("hey_livekit.onnx")], 48000).unwrap();
    let score = model.predict_clip(&upsampled).unwrap()["hey_livekit"];
    assert!(score >= THRESHOLD, "48 kHz positive.wav scored {score}");
}

#[test]
fn test_load_and_unload_model() {
    let no_models: &[PathBuf] = &[];
    let mut model = WakeWordModel::new(no_models, SAMPLE_RATE as u32).unwrap();
    assert!(model.predict(&[0; 32000]).unwrap().is_empty());

    model.load_model(fixture("hey_livekit.onnx"), Some("custom")).unwrap();
    assert_eq!(model.model_names().collect::<Vec<_>>(), ["custom"]);
    assert!(model.predict_clip(&[0; 32000]).unwrap().contains_key("custom"));

    assert!(model.unload_model("custom"));
    assert!(!model.unload_model("custom"));
    assert_eq!(model.model_names().count(), 0);
}

#[test]
fn test_rejects_non_streaming_model() {
    let result = WakeWordModel::new(&[fixture("not_streaming.onnx")], SAMPLE_RATE as u32);
    assert!(matches!(result, Err(WakeWordError::NotStreamingModel(_))));

    let result = WakeWordModel::new(&[fixture("missing.onnx")], SAMPLE_RATE as u32);
    assert!(matches!(result, Err(WakeWordError::ModelNotFound(_))));
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

    let mut default = WakeWordModel::new(&[fixture("hey_livekit.onnx")], sample_rate).unwrap();
    let mut tuned =
        WakeWordModel::with_session_options(&[fixture("hey_livekit.onnx")], sample_rate, options)
            .unwrap();

    assert_eq!(
        default.predict_clip(&samples).unwrap()["hey_livekit"],
        tuned.predict_clip(&samples).unwrap()["hey_livekit"]
    );
}

/// Every optimization level builds a model that still detects the wake word.
#[test]
fn test_optimization_levels() {
    let (sample_rate, samples) = common::read_wav("positive.wav");

    for level in [
        GraphOptimizationLevel::Disable,
        GraphOptimizationLevel::Level1,
        GraphOptimizationLevel::Level3,
    ] {
        let options = SessionOptions { optimization_level: level, ..Default::default() };
        let mut model = WakeWordModel::with_session_options(
            &[fixture("hey_livekit.onnx")],
            sample_rate,
            options,
        )
        .unwrap();
        let score = model.predict_clip(&samples).unwrap()["hey_livekit"];
        assert!(score > THRESHOLD, "{level:?} scored {score}");
    }
}

/// A model loaded after construction inherits the session options.
#[test]
fn test_load_model_inherits_session_options() {
    let (sample_rate, samples) = common::read_wav("positive.wav");

    let no_models: &[PathBuf] = &[];
    let options =
        SessionOptions { optimization_level: GraphOptimizationLevel::Level1, ..Default::default() };
    let mut model = WakeWordModel::with_session_options(no_models, sample_rate, options).unwrap();
    model.load_model(fixture("hey_livekit.onnx"), Some("late")).unwrap();

    assert!(model.predict_clip(&samples).unwrap()["late"] > THRESHOLD);
}
