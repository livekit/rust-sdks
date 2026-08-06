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

// PlatformAudio exerciser. Demonstrates the PlatformAudio API and verifies
// the AdmProxy worker-thread marshaling, all without a LiveKit server:
//
// - acquire/release lifecycle and full runtime teardown/reacquire cycles
// - device enumeration, selection, and hot-swap while recording
// - recording start/stop on real hardware
// - audio processing (AEC/AGC/NS) configuration
// - heavy concurrent access from many threads, the access pattern that
//   exercises the proxy's marshaling onto the WebRTC worker thread
//
// Run: cargo run -p livekit --example platform_audio
// Success criteria: prints ALL PHASES PASSED without hang, crash, or errors.
// Requires audio hardware and microphone permission, so it is an example
// rather than a CI test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use livekit::{AudioProcessingOptions, PlatformAudio};
use log::{Level, LevelFilter, Metadata, Record};

// Counts LkRuntime teardowns so the test can assert that dropping the last
// PlatformAudio really destroys the factory and the AdmProxy with it
static RUNTIME_DROPS: AtomicUsize = AtomicUsize::new(0);

struct StdoutLogger;

impl log::Log for StdoutLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }
    fn log(&self, record: &Record) {
        let msg = record.args().to_string();
        if msg.contains("LkRuntime::drop") {
            RUNTIME_DROPS.fetch_add(1, Ordering::Relaxed);
        }
        if record.level() <= Level::Info {
            println!("[{}] {}", record.level(), msg);
        }
    }
    fn flush(&self) {}
}

static LOGGER: StdoutLogger = StdoutLogger;

fn main() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(LevelFilter::Debug);

    println!("=== Phase 1: basic lifecycle ===");
    let audio = PlatformAudio::new().expect("PlatformAudio::new() failed");
    println!("ref_count after new: {}", audio.ref_count());

    let rec_devices: Vec<_> = audio.recording_devices().collect();
    let play_devices: Vec<_> = audio.playout_devices().collect();
    println!("recording devices ({}):", rec_devices.len());
    for d in &rec_devices {
        println!("  [{}] {} ({})", d.index, d.name, d.id.as_str());
    }
    println!("playout devices ({}):", play_devices.len());
    for d in &play_devices {
        println!("  [{}] {} ({})", d.index, d.name, d.id.as_str());
    }
    assert!(!rec_devices.is_empty(), "no recording devices found");
    assert!(!play_devices.is_empty(), "no playout devices found");

    println!(
        "hardware aec/agc/ns available: {}/{}/{}",
        audio.is_hardware_aec_available(),
        audio.is_hardware_agc_available(),
        audio.is_hardware_ns_available()
    );
    println!(
        "active aec/agc/ns type: {:?}/{:?}/{:?}",
        audio.active_aec_type(),
        audio.active_agc_type(),
        audio.active_ns_type()
    );

    println!("=== Phase 2: device selection + recording ===");
    audio.set_recording_device(&rec_devices[0].id).expect("set_recording_device");
    audio.set_playout_device(&play_devices[0].id).expect("set_playout_device");
    // Exercises the by-guid lookup plus the stop/init/start dance (the
    // start branch only runs when playout is live, which needs a room)
    audio.switch_playout_device(&play_devices[0].id).expect("switch_playout_device");

    match audio.start_recording() {
        Ok(()) => {
            println!("recording started (initialized: {})", audio.is_recording_initialized());
            thread::sleep(Duration::from_millis(300));
            // Hot-swap while recording if a second device exists
            if rec_devices.len() > 1 {
                println!("switching recording device while recording...");
                audio.switch_recording_device(&rec_devices[1].id).expect("switch_recording_device");
                thread::sleep(Duration::from_millis(300));
                audio.switch_recording_device(&rec_devices[0].id).expect("switch back");
            }
            audio.stop_recording().expect("stop_recording");
            println!("recording stopped");
        }
        Err(e) => {
            // Mic permission may be denied for the terminal, still a valid
            // control-plane exercise, but report it
            println!("start_recording failed (mic permission?): {e}");
        }
    }

    println!("=== Phase 3: audio processing reconfiguration ===");
    for prefer_hw in [true, false, true] {
        audio
            .configure_audio_processing(AudioProcessingOptions {
                echo_cancellation: true,
                noise_suppression: true,
                auto_gain_control: true,
                prefer_hardware_processing: prefer_hw,
            })
            .expect("configure_audio_processing");
    }
    audio.set_echo_cancellation(false, false).expect("set_echo_cancellation");
    audio.set_echo_cancellation(true, true).expect("set_echo_cancellation");

    println!("=== Phase 4: concurrent hammering (16 threads x 50 iterations) ===");
    let errors = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    let mut handles = Vec::new();
    for t in 0..16usize {
        let audio = audio.clone();
        let errors = errors.clone();
        handles.push(thread::spawn(move || {
            for i in 0..50usize {
                match t % 4 {
                    0 => {
                        // enumerate
                        let n = audio.recording_devices().count();
                        if n == 0 {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                        let _ = audio.playout_devices().count();
                    }
                    1 => {
                        // getters
                        let _ = audio.ref_count();
                        let _ = audio.is_recording_initialized();
                        let _ = audio.is_hardware_aec_available();
                        let _ = audio.active_aec_type();
                    }
                    2 => {
                        // recording start/stop churn
                        let _ = audio.start_recording();
                        let _ = audio.stop_recording();
                    }
                    _ => {
                        // acquire/release churn: extra instances created and
                        // dropped concurrently, forcing mode switches
                        if let Ok(extra) = PlatformAudio::new() {
                            let _ = extra.ref_count();
                            drop(extra);
                        } else {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                if i % 25 == 0 {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }));
    }
    for h in handles {
        h.join().expect("worker thread panicked");
    }
    println!(
        "concurrent phase done in {:?}, errors: {}",
        start.elapsed(),
        errors.load(Ordering::Relaxed)
    );
    assert_eq!(errors.load(Ordering::Relaxed), 0, "errors during concurrent phase");

    println!("=== Phase 5: full runtime teardown / reacquire churn ===");
    // With no rooms alive, dropping the last PlatformAudio destroys the whole
    // LkRuntime (factory + AdmProxy), so each iteration exercises AdmProxy
    // construction on the worker and destruction initiated from this thread
    drop(audio);
    let drops_before = RUNTIME_DROPS.load(Ordering::Relaxed);
    for i in 0..5 {
        let a = PlatformAudio::new().expect("reacquire");
        assert_eq!(a.ref_count(), 1, "iteration {i}: expected sole owner");
        let _ = a.start_recording();
        if i % 2 == 0 {
            // Drop while recording is active: release must stop recording
            // via SwitchRecordingAdm before the runtime is torn down
        } else {
            let _ = a.stop_recording();
        }
        drop(a);
    }
    let teardowns = RUNTIME_DROPS.load(Ordering::Relaxed) - drops_before;
    println!("full runtime teardowns in phase 5: {teardowns}");
    assert!(teardowns >= 5, "expected >= 5 LkRuntime teardowns, got {teardowns}");

    println!("ALL PHASES PASSED");
}
