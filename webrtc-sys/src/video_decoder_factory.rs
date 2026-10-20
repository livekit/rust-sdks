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

//! Tests for the native video decoders behind `livekit_ffi::VideoDecoderFactory`.
//!
//! The Jetson tests skip themselves on hosts or builds without the Jetson
//! hardware decoder.

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Mutex, MutexGuard, PoisonError},
    };

    #[cxx::bridge(namespace = "livekit_ffi")]
    pub mod ffi_tests {
        unsafe extern "C++" {
            include!("livekit/video_decoder_factory.h");

            fn jetson_h265_decode_for_test(
                access_units: &[u8],
                corrupt_index: u32,
                release_immediately: bool,
            ) -> Vec<u32>;
        }
    }

    // Statuses, mirrored from video_decoder_factory.h.
    const UNSUPPORTED: u32 = 0;
    const OK: u32 = 1;

    /// Six access units at 320x240 (key, delta, delta, key, delta, delta),
    /// then the same at 256x192, each prefixed with its little-endian u32
    /// size. Generated with GStreamer `x265enc tune=zerolatency
    /// speed-preset=ultrafast key-int-max=3` and `h265parse`.
    const STREAM: &[u8] = include_bytes!("jetson/testdata/h265_two_resolutions.bin");
    const ACCESS_UNITS: u32 = 12;
    const NO_CORRUPTION: u32 = u32::MAX;
    const CAPTURE_THREAD_NAME: &str = "lk-jetson-dec";

    /// Serializes the decoder tests, so thread and descriptor counts only
    /// see one decoder at a time.
    static DECODER: Mutex<()> = Mutex::new(());

    /// Locks [`DECODER`], ignoring poisoning so one failure is reported once.
    fn lock_decoder() -> MutexGuard<'static, ()> {
        DECODER.lock().unwrap_or_else(PoisonError::into_inner)
    }

    struct Decoded {
        accepted: u32,
        rejected: u32,
        /// (width, height, RTP timestamp) per picture, in delivery order.
        pictures: Vec<(u32, u32, u32)>,
    }

    /// Returns `None` when there is no Jetson hardware decoder to test.
    fn decode(stream: &[u8], corrupt_index: u32, release_immediately: bool) -> Option<Decoded> {
        let result =
            ffi_tests::jetson_h265_decode_for_test(stream, corrupt_index, release_immediately);
        match result[0] {
            UNSUPPORTED => {
                eprintln!("skipping: no Jetson hardware H.265 decoder");
                None
            }
            OK => Some(Decoded {
                accepted: result[1],
                rejected: result[2],
                pictures: result[3..].chunks_exact(3).map(|p| (p[0], p[1], p[2])).collect(),
            }),
            status => panic!("Jetson decoder failed to configure (status {status})"),
        }
    }

    /// RTP timestamp the test hook gives the access unit at `index`.
    fn rtp(index: u32) -> u32 {
        (index + 1) * 3000
    }

    /// The fixture without its first `skip` access units.
    fn skipping(skip: usize) -> Vec<u8> {
        let mut rest = STREAM;
        for _ in 0..skip {
            let size = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
            rest = &rest[4 + size..];
        }
        rest.to_vec()
    }

    fn open_fds() -> usize {
        fs::read_dir("/proc/self/fd").unwrap().count()
    }

    fn capture_threads() -> usize {
        fs::read_dir("/proc/self/task")
            .unwrap()
            .filter_map(|task| fs::read_to_string(task.ok()?.path().join("comm")).ok())
            .filter(|name| name.trim_end() == CAPTURE_THREAD_NAME)
            .count()
    }

    #[test]
    fn decodes_through_a_resolution_change() {
        let _decoder = lock_decoder();
        let Some(decoded) = decode(STREAM, NO_CORRUPTION, false) else { return };

        assert_eq!((decoded.accepted, decoded.rejected), (ACCESS_UNITS, 0));
        let expected: Vec<_> = (0..ACCESS_UNITS)
            .map(|i| if i < 6 { (320, 240, rtp(i)) } else { (256, 192, rtp(i)) })
            .collect();
        assert_eq!(decoded.pictures, expected);
    }

    #[test]
    fn refuses_delta_frames_until_a_keyframe() {
        let _decoder = lock_decoder();
        // Starts on the two delta frames that follow the first keyframe.
        let Some(decoded) = decode(&skipping(1), NO_CORRUPTION, false) else { return };

        assert_eq!((decoded.accepted, decoded.rejected), (ACCESS_UNITS - 3, 2));
        let timestamps: Vec<_> = decoded.pictures.iter().map(|p| p.2).collect();
        assert_eq!(timestamps, (2..ACCESS_UNITS - 1).map(rtp).collect::<Vec<_>>());
    }

    #[test]
    fn recovers_from_corrupt_input_at_the_next_keyframe() {
        let _decoder = lock_decoder();
        // Scrambles the first delta frame; the next keyframe is index 3.
        let Some(decoded) = decode(STREAM, 1, false) else { return };

        assert_eq!(decoded.accepted + decoded.rejected, ACCESS_UNITS);
        let after_keyframe: Vec<_> =
            decoded.pictures.iter().map(|p| p.2).filter(|&ts| ts >= rtp(3)).collect();
        assert_eq!(after_keyframe, (3..ACCESS_UNITS).map(rtp).collect::<Vec<_>>());
    }

    #[test]
    fn release_stops_the_capture_thread_and_frees_descriptors() {
        let _decoder = lock_decoder();
        // The first decoder loads the NVIDIA libraries, which keep some
        // device descriptors open for the life of the process.
        if decode(STREAM, NO_CORRUPTION, false).is_none() {
            return;
        }
        let baseline_fds = open_fds();

        // Alternate between releasing after every picture arrived and
        // releasing while pictures are still in flight.
        for release_immediately in [false, true, false, true, false, true] {
            decode(STREAM, NO_CORRUPTION, release_immediately).unwrap();
            assert_eq!(capture_threads(), 0, "capture thread outlived Release()");
        }
        assert_eq!(open_fds(), baseline_fds, "decoder leaked file descriptors");
    }
}
