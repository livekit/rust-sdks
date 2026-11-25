// Copyright 2025 LiveKit, Inc.
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

//! Opus encoding and decoding through libwebrtc's builtin audio codec
//! factories.

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug, Clone)]
    pub struct OpusEncoderSettings {
        /// Input sample rate.
        pub sample_rate: u32,
        /// Input channel count (1 or 2).
        pub num_channels: u32,
        /// Encode a stereo signal.
        pub stereo: bool,
        /// Target bitrate, or 0 for the Opus default.
        pub bitrate_bps: u32,
        pub dtx: bool,
        pub fec: bool,
        /// Packet duration (10, 20, 40 or 60), or 0 for 20 ms.
        pub frame_duration_ms: u32,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct OpusEncodeResult {
        /// RFC 6464 packet level, including the encoder voice activity bit.
        pub audio_level: u8,
        /// `false` when the input was not 10 ms of audio.
        pub ok: bool,
        /// Whether a packet was completed and written out.
        pub has_packet: bool,
        /// Timestamp of the packet's first sample, in 48 kHz units.
        pub timestamp: u32,
        /// Packet duration in 48 kHz samples per channel.
        pub samples_per_channel: u32,
    }

    unsafe extern "C++" {
        include!("livekit/audio_codec.h");

        type NativeOpusEncoder;

        fn create_opus_encoder(
            settings: &OpusEncoderSettings,
        ) -> Result<UniquePtr<NativeOpusEncoder>>;
        /// Encodes 10 ms of interleaved PCM; see `packet` when it completes
        /// one.
        fn encode(self: Pin<&mut NativeOpusEncoder>, pcm: &[i16]) -> OpusEncodeResult;
        /// The packet the last `encode` completed.
        fn packet(self: &NativeOpusEncoder) -> &[u8];
        fn set_bitrate(self: Pin<&mut NativeOpusEncoder>, bitrate_bps: u32);
        fn set_packet_loss_rate(self: Pin<&mut NativeOpusEncoder>, fraction: f32);
        fn bitrate(self: &NativeOpusEncoder) -> u32;

        type NativeOpusDecoder;

        fn create_opus_decoder(
            sample_rate: u32,
            num_channels: u32,
        ) -> Result<UniquePtr<NativeOpusDecoder>>;
        /// Decodes one packet, replacing the contents of `pcm`.
        /// Decodes one packet; see `output`.
        fn decode(self: Pin<&mut NativeOpusDecoder>, packet: &[u8]) -> bool;
        /// Conceals one lost packet, using the in-band FEC of `next_packet`
        /// when it has some and packet loss concealment otherwise; see
        /// `output`.
        fn conceal(self: Pin<&mut NativeOpusDecoder>, next_packet: &[u8]) -> bool;
        /// Interleaved PCM of the last successful `decode` or `conceal`.
        fn output(self: &NativeOpusDecoder) -> &[i16];
    }
}

// SAFETY: the Opus encoder and decoder have no thread affinity; they are only
// used through `Pin<&mut _>` / `&_` borrows of their single owner.
impl_thread_safety!(ffi::NativeOpusEncoder, Send);
impl_thread_safety!(ffi::NativeOpusDecoder, Send);
