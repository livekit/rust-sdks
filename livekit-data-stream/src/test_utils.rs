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

use rand::{rngs::StdRng, Rng, SeedableRng};

/// Fixed RNG seed that keeps output identical on every run.
const RANDOM_SEED: u64 = 0x1234_5678_9abc_def0;

/// Deterministic, barely-compressible lowercase text (so its deflate output spans chunks).
///
/// Note that this IS still compressible - it is ascii text, not just random noise.
///
/// Seeded with a fixed value so the output is identical on every run; the letters carry
/// enough entropy that deflate can't shrink them away, unlike repetitive text ("aaaa...").
#[cfg(any(test, feature = "test-utils"))]
pub fn pseudo_random_text(size_bytes: usize) -> String {
    let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
    (0..size_bytes).map(|_| rng.random_range(b'a'..=b'z') as char).collect()
}

/// Generate deterministic, somewhat-compressible text (repeated marker + pseudo-random
/// lowercase).
///
/// 50kb will compresses to still be >15 KB (so it can't inline) but well under its raw 50kb size.
///
/// Seeded with a fixed value so the output is identical on every run.
#[cfg(any(test, feature = "test-utils"))]
pub fn somewhat_compressible(size_bytes: usize) -> String {
    let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
    let mut s = String::new();

    let interstitial = "hello world";
    let blocks = size_bytes.div_ceil(interstitial.len() + 1000);
    for _ in 0..blocks {
        // Inserting the interstitial periodically ensures there is repeating bytes which can be
        // compressed among the randomness.
        s.push_str(interstitial);
        for _ in 0..1000 {
            s.push(rng.random_range(b'a'..=b'z') as char);
        }
    }

    s.truncate(size_bytes);
    s
}
