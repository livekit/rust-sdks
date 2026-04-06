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

use hound::WavReader;
use std::path::PathBuf;

/// Reads a WAV file from the fixtures directory, returning its sample rate
/// and samples as i16 PCM (mono, first channel only if stereo).
pub fn read_wav(name: &str) -> (u32, Vec<i16>) {
    let path = fixtures_dir().join(name);
    let mut reader = WavReader::open(&path)
        .unwrap_or_else(|e| panic!("Failed to open {}: {}", path.display(), e));
    let spec = reader.spec();
    println!(
        "{}: {} Hz, {} channels, {} bits per sample",
        name, spec.sample_rate, spec.channels, spec.bits_per_sample
    );

    let all_samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();

    // Down-mix to mono by taking every Nth sample (first channel)
    let samples = if spec.channels > 1 {
        all_samples.chunks(spec.channels as usize).map(|chunk| chunk[0]).collect()
    } else {
        all_samples
    };

    (spec.sample_rate, samples)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}
