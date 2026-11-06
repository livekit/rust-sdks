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

use soxr_sys::*;
use std::{ptr, sync::Barrier, thread};

const RATES: [(u32, u32); 8] = [
    (48000, 16000),
    (24000, 48000),
    (44100, 48000),
    (96000, 44100),
    (48000, 24000),
    (16000, 48000),
    (48000, 44100),
    (44100, 16000),
];

/// Owns one resampler on the calling thread, including cleanup on assertion failure.
struct Resampler(soxr_t);

impl Drop for Resampler {
    fn drop(&mut self) {
        // SAFETY: This handle was returned by soxr_create and has a single owner.
        unsafe { soxr_delete(self.0) };
    }
}

/// Converts a nonzero signal and drains the filter's delayed samples.
fn resample(case: usize) -> Vec<f32> {
    let (input_rate, output_rate) = RATES[case % RATES.len()];
    let input: Vec<f32> = (0..input_rate / 10).map(|i| 0.5 * (i as f32 * 0.17).sin()).collect();
    let mut output = vec![0.0f32; output_rate as usize / 10 + 1024];
    let mut error = ptr::null();
    // SAFETY: These functions return value-only configuration structs.
    let (io, quality, runtime) = unsafe {
        let recipe = [SOXR_MQ, SOXR_20_BITQ, SOXR_28_BITQ][case % 3]
            | [SOXR_LINEAR_PHASE, SOXR_INTERMEDIATE_PHASE, SOXR_MINIMUM_PHASE][case / 3 % 3];
        (
            soxr_io_spec(soxr_datatype_t_SOXR_FLOAT32_I, soxr_datatype_t_SOXR_FLOAT32_I),
            soxr_quality_spec(recipe.into(), 0),
            soxr_runtime_spec(1),
        )
    };
    // SAFETY: All configuration/error pointers are live for the call; rates and
    // channel count are valid. Each worker constructs its own independent handle.
    let handle = unsafe {
        soxr_create(input_rate.into(), output_rate.into(), 1, &mut error, &io, &quality, &runtime)
    };
    assert!(!handle.is_null());
    let resampler = Resampler(handle);
    assert!(error.is_null());
    let (mut consumed, mut produced, mut flushed) = (0, 0, 0);
    // SAFETY: The mono float buffers have the supplied sample capacities and
    // remain live; only this thread uses the resampler.
    let error = unsafe {
        soxr_process(
            resampler.0,
            input.as_ptr().cast(),
            input.len(),
            &mut consumed,
            output.as_mut_ptr().cast(),
            output.len(),
            &mut produced,
        )
    };
    assert!(error.is_null());
    assert_eq!(consumed, input.len());
    assert!(produced <= output.len());
    // SAFETY: Null input requests a flush; the remaining output slice is live
    // and has the supplied capacity. No other thread accesses this handle.
    let error = unsafe {
        soxr_process(
            resampler.0,
            ptr::null(),
            0,
            ptr::null_mut(),
            output[produced..].as_mut_ptr().cast(),
            output.len() - produced,
            &mut flushed,
        )
    };
    assert!(error.is_null());
    assert_eq!(produced + flushed, output_rate as usize / 10);
    output.truncate(produced + flushed);
    assert!(output.iter().all(|sample| sample.is_finite() && sample.abs() < 1.0));
    output
}

/// Exercises simultaneous first use, cache growth and processing on independent streams.
#[test]
fn concurrent_resamplers_match_serial_output() {
    const THREADS: usize = 16;
    const ROUNDS: usize = 12;
    let start = Barrier::new(THREADS);
    let results = thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|worker| {
                let start = &start;
                scope.spawn(move || {
                    // Start before the first resampler initializes the global FFT caches.
                    start.wait();
                    (0..ROUNDS)
                        .map(|round| {
                            let case = worker + round * THREADS;
                            (case, resample(case))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>()
    });
    // Generate references afterwards so they cannot warm the caches before the race.
    for (case, concurrent) in results.into_iter().flatten() {
        assert_eq!(concurrent, resample(case), "resampler case {case}");
    }
}
