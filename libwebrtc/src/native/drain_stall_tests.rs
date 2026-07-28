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

//! Regression tests for the buffered `NativeAudioSource::capture_frame` drain stall
//! (rust-sdks #408 / #420 / #497): the C++ `AudioTrackSource` drain fires the per-chunk completion
//! that `capture_frame` awaits. If the drain stops progressing — a sink blocked in `OnData`, or the
//! drain TaskQueue CPU-starved — an unbounded await would wedge the producer (and any session built
//! on it) forever.

use crate::audio_frame::AudioFrame;
use crate::audio_source::native::NativeAudioSource;
use crate::audio_source::AudioSourceOptions;

const SAMPLE_RATE: u32 = 48000;

fn silent_frame<'a>(samples: usize) -> AudioFrame<'a> {
    AudioFrame {
        data: vec![0i16; samples].into(),
        sample_rate: SAMPLE_RATE,
        num_channels: 1,
        samples_per_channel: samples as u32,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_frame_completes_under_normal_backpressure() {
    // The fix must not regress the healthy path. A large queue keeps the derived timeout far above
    // the ~one-queue-duration a healthy deferral needs, so this asserts "no false-trip on legitimate
    // backpressure" without coupling to the production floor (which would make it flaky on a loaded
    // CI runner). No sink/track is created, so this is safe on every platform.
    const QUEUE_MS: u32 = 1000;
    let q = (SAMPLE_RATE / 1000 * QUEUE_MS) as usize;
    let source = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, QUEUE_MS);

    for i in 0..3u32 {
        // q + q/2 forces a deferral (completion routed through the drain) every call.
        source
            .capture_frame(&silent_frame(q + q / 2))
            .await
            .unwrap_or_else(|e| panic!("healthy capture {i} failed: {e:?}"));
    }
}

// The stall reproducer needs a real audio track, which requires a `PeerConnectionFactory`. On
// macOS/Windows that brings up the platform AudioDeviceModule, whose real-time audio thread aborts
// the process when the test's blocking sink stalls it. The Linux CI webrtc build has no such device
// backend, so the reproducer runs there and is deterministic. The fix itself is platform-independent
// and the healthy-path test above runs everywhere. This test also creates a factory, so — like the
// sibling factory tests — it relies on serial execution (CI runs `--test-threads=1`).
#[cfg(target_os = "linux")]
mod stall {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::mpsc;
    use webrtc_sys::audio_track as sys_at;

    use super::{silent_frame, SAMPLE_RATE};
    use crate::audio_source::native::NativeAudioSource;
    use crate::audio_source::AudioSourceOptions;
    use crate::peer_connection_factory::native::PeerConnectionFactoryExt;
    use crate::peer_connection_factory::PeerConnectionFactory;
    use crate::RtcErrorType;

    /// A sink whose `on_data` blocks the C++ 10ms drain task, modelling a wedged/starved consumer.
    /// It signals `entered` on its first call (so the test awaits the stall instead of sleeping for
    /// it) and unblocks when `released` is set.
    struct BlockingSink {
        entered: mpsc::UnboundedSender<()>,
        released: Arc<AtomicBool>,
    }

    impl sys_at::AudioSink for BlockingSink {
        fn on_data(
            &self,
            _data: &[i16],
            _sample_rate: i32,
            _num_channels: usize,
            _num_frames: usize,
        ) {
            let _ = self.entered.send(());
            while !self.released.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    // worker_threads = 2: one for the (on a revert, possibly-wedged) capture task, one for the timeout.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn capture_frame_recovers_when_drain_stalls() {
        const QUEUE_MS: u32 = 200;
        let q = (SAMPLE_RATE / 1000 * QUEUE_MS) as usize;

        let factory = PeerConnectionFactory::default();
        let source =
            NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, QUEUE_MS);
        let track = factory.create_audio_track("drain-stall", source.clone());

        let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
        let released = Arc::new(AtomicBool::new(false));
        let native_sink = sys_at::ffi::new_native_audio_sink(
            Box::new(sys_at::AudioSinkWrapper::new(Arc::new(BlockingSink {
                entered: entered_tx,
                released: released.clone(),
            }))),
            SAMPLE_RATE as i32,
            1,
        );
        let audio = unsafe { sys_at::ffi::media_to_audio(track.sys_handle()) };
        audio.add_sink(&native_sink);

        // Wait until the drain is actually stalled inside the sink (bounded, so a wiring regression
        // fails the test cleanly instead of hanging the CI job).
        tokio::time::timeout(Duration::from_secs(5), entered_rx.recv())
            .await
            .expect("drain never entered the sink within 5s")
            .expect("sink signal channel closed");

        // Feed 2x the queue so the second chunk must wait on the (now stalled) drain.
        let src = source.clone();
        let mut handle = tokio::spawn(async move { src.capture_frame(&silent_frame(q * 2)).await });

        // Outer bound distinguishes "returned" from "hung", well above the fix's own timeout.
        let res = tokio::time::timeout(Duration::from_secs(8), &mut handle).await;

        // Release the drain regardless, so teardown and the recovery check below can proceed.
        released.store(true, Ordering::Release);
        if res.is_err() {
            let _ = handle.await;
        }

        match res {
            Ok(Ok(Err(e))) => {
                assert_eq!(e.error_type, RtcErrorType::InvalidState);
                assert!(
                    e.message.contains("source drain stalled"),
                    "unexpected error: {}",
                    e.message
                );
            }
            other => {
                panic!("capture_frame should return an error on a stalled drain, got: {other:?}")
            }
        }

        // The stall must leave the source usable: the timeout path releases the pending completion
        // rather than poisoning the slot, so a fresh capture succeeds once the drain is unstuck.
        tokio::time::timeout(Duration::from_secs(5), source.capture_frame(&silent_frame(q)))
            .await
            .expect("recovery capture hung")
            .expect("source did not recover after the stall cleared");

        audio.remove_sink(&native_sink);
    }
}
