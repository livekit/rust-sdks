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

//! Tests for the pass-through video decoder (`passthrough_video_decoder.cpp`).
//! The decoder has no Rust surface of its own; it is selected through
//! [`crate::peer_connection_factory`] and its frames are read through
//! [`crate::video_frame_buffer`].

#[cfg(test)]
mod tests {
    #[cxx::bridge(namespace = "livekit_ffi")]
    pub mod ffi_tests {
        /// What one `Decode()` call produced.
        #[derive(Debug)]
        struct DecodeForTest {
            emitted: bool,
            rtp_timestamp_matches: bool,
            shares_payload: bool,
            return_code: i32,
            width: i32,
            height: i32,
            sequence: u64,
            keyframe: bool,
        }

        unsafe extern "C++" {
            include!("livekit/passthrough_video_decoder.h");

            /// Drives a `PassthroughVideoDecoder` with `script` (op codes mirrored
            /// below). Ops that decode use `payload` as the access unit.
            fn passthrough_decode_for_test(
                codec: &str,
                script: &[u8],
                payload: &[u8],
            ) -> Vec<DecodeForTest>;

            /// `implementation_name` and `is_hardware_accelerated` of the decoder.
            fn passthrough_decoder_info_for_test() -> String;

            /// Stream identities of two decoder instances, each after one decode.
            fn passthrough_decoder_identities_for_test() -> Vec<usize>;
        }
    }

    // Script op codes, mirrored from passthrough_video_decoder.h.
    const KEY_64X64: u8 = 0;
    const KEY_128X72: u8 = 1;
    const KEY_NO_DIMS: u8 = 2;
    const DELTA: u8 = 3;
    const SET_KEYFRAME_FLAG: u8 = 4;

    // Return codes from modules/video_coding/include/video_error_codes.h.
    const OK: i32 = 0;
    const OK_REQUEST_KEYFRAME: i32 = 4;

    include!("../test/fixtures/passthrough_streams.rs");

    const PAYLOAD: &[u8] = &[0, 0, 0, 1, 0x65, 0x88, 0x84];

    /// Every `Decode()` emits exactly one frame, synchronously, carrying the
    /// input's RTP timestamp.
    #[test]
    fn passthrough_emits_every_frame_with_input_rtp_timestamp() {
        let results = ffi_tests::passthrough_decode_for_test(
            "H264",
            &[KEY_64X64, DELTA, DELTA, DELTA],
            PAYLOAD,
        );
        assert_eq!(results.len(), 4);
        for r in &results {
            assert!(r.emitted, "{r:?}");
            assert!(r.rtp_timestamp_matches, "{r:?}");
            assert_eq!(r.return_code, OK, "{r:?}");
        }
        assert!(results[0].keyframe);
        assert!(results[1..].iter().all(|r| !r.keyframe));
    }

    /// The emitted buffer shares the received payload instead of copying it.
    #[test]
    fn passthrough_shares_payload_without_copy() {
        let results = ffi_tests::passthrough_decode_for_test("H264", &[KEY_64X64, DELTA], PAYLOAD);
        assert!(results.iter().all(|r| r.shares_payload), "{results:?}");
    }

    /// Delta frames carry no dimensions; they report the last keyframe's.
    #[test]
    fn delta_frames_inherit_last_keyframe_dimensions() {
        let results =
            ffi_tests::passthrough_decode_for_test("H264", &[KEY_64X64, DELTA, DELTA], PAYLOAD);
        for r in &results {
            assert_eq!((r.width, r.height), (64, 64), "{r:?}");
        }
    }

    /// A keyframe at a new size updates the dimensions without changing the
    /// decoder's stream identity or breaking its sequence (libwebrtc does not
    /// reconfigure a decoder on resolution change).
    #[test]
    fn keyframe_with_new_dimensions_updates_cache_without_new_identity() {
        let results = ffi_tests::passthrough_decode_for_test(
            "H264",
            &[KEY_64X64, DELTA, KEY_128X72, DELTA],
            PAYLOAD,
        );
        let dims: Vec<_> = results.iter().map(|r| (r.width, r.height)).collect();
        assert_eq!(dims, [(64, 64), (64, 64), (128, 72), (128, 72)]);
        let sequence: Vec<_> = results.iter().map(|r| r.sequence).collect();
        assert_eq!(sequence, [0, 1, 2, 3]);
    }

    /// AV1 keyframes arrive without dimensions; they come from the sequence
    /// header.
    #[test]
    fn av1_dimensions_come_from_sequence_header() {
        let (_, keyframe) = AV1.units[0];
        let results = ffi_tests::passthrough_decode_for_test("AV1", &[KEY_NO_DIMS], keyframe);
        assert_eq!((results[0].width, results[0].height), (AV1.width as i32, AV1.height as i32));
    }

    /// A raised keyframe flag makes the next `Decode()` return
    /// `OK_REQUEST_KEYFRAME` once; the frame is still emitted.
    #[test]
    fn keyframe_flag_returns_ok_request_keyframe_once() {
        let results = ffi_tests::passthrough_decode_for_test(
            "H264",
            &[KEY_64X64, SET_KEYFRAME_FLAG, DELTA, DELTA],
            PAYLOAD,
        );
        let codes: Vec<_> = results.iter().map(|r| r.return_code).collect();
        assert_eq!(codes, [OK, OK_REQUEST_KEYFRAME, OK]);
        assert!(results.iter().all(|r| r.emitted));
    }

    /// Each decoder instance numbers its frames from 0 under its own identity.
    #[test]
    fn sequence_numbers_are_contiguous_per_instance() {
        let ids = ffi_tests::passthrough_decoder_identities_for_test();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], 0);
        assert_ne!(ids[0], ids[1]);
        let results =
            ffi_tests::passthrough_decode_for_test("VP8", &[KEY_64X64, DELTA, DELTA], PAYLOAD);
        let sequence: Vec<_> = results.iter().map(|r| r.sequence).collect();
        assert_eq!(sequence, [0, 1, 2]);
    }

    /// The decoder identifies itself as a software pass-through.
    #[test]
    fn decoder_info_reports_passthrough() {
        assert_eq!(ffi_tests::passthrough_decoder_info_for_test(), "LiveKit encoded passthrough|0");
    }
}
