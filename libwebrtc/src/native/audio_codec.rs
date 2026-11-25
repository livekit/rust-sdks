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

//! Standalone Opus encoding and decoding, outside of a PeerConnection, using
//! libwebrtc's builtin audio codecs.

use cxx::UniquePtr;
use thiserror::Error;
use webrtc_sys::audio_codec as ac_sys;

use crate::audio_frame::AudioFrame;

/// Clock rate of Opus timestamps and durations.
pub const OPUS_CLOCK_RATE: u32 = 48_000;

/// Configuration of an [`OpusEncoder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusEncoderConfig {
    /// Sample rate of the input, a multiple of 100 between 8 and 96 kHz.
    pub sample_rate: u32,
    /// Channel count of the input (1 or 2).
    pub num_channels: u32,
    /// Encode a stereo signal; a mono input is upmixed.
    pub stereo: bool,
    /// Target bitrate in bits per second, or `0` for the Opus default.
    pub bitrate_bps: u32,
    /// Discontinuous transmission: send almost nothing during silence.
    pub dtx: bool,
    /// In-band forward error correction, which takes effect once a packet
    /// loss rate is set with [`OpusEncoder::set_packet_loss_rate`].
    pub fec: bool,
    /// Packet duration in milliseconds: 10, 20, 40 or 60.
    pub frame_duration_ms: u32,
}

impl Default for OpusEncoderConfig {
    /// 48 kHz mono input, default bitrate, 20 ms packets, FEC on, DTX off.
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            num_channels: 1,
            stereo: false,
            bitrate_bps: 0,
            dtx: false,
            fec: true,
            frame_duration_ms: 20,
        }
    }
}

/// One Opus packet produced by an [`OpusEncoder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusPacket {
    /// RFC 6464 packet level in the low seven bits and encoder voice activity in bit 7.
    pub audio_level: u8,
    /// The Opus packet.
    pub data: Vec<u8>,
    /// Position of the packet's first sample, in 48 kHz samples since the
    /// first encoded frame (an RTP-style timestamp, wrapping at `u32::MAX`).
    pub timestamp: u32,
    /// Duration of the packet in 48 kHz samples per channel.
    pub samples_per_channel: u32,
}

/// Error creating an [`OpusEncoder`] or [`OpusDecoder`].
#[derive(Debug, Error)]
#[error("failed to create Opus codec: {reason}")]
pub struct OpusCreateError {
    /// Why the codec could not be created.
    pub reason: String,
}

/// Error returned by [`OpusEncoder::encode`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OpusEncodeError {
    /// The frame is not 10 ms of audio in the configured input format.
    #[error("expected 10 ms of audio in the configured input format")]
    InvalidFrame,
}

/// Error returned by [`OpusDecoder::decode`] and [`OpusDecoder::conceal`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum OpusDecodeError {
    /// The packet is empty or could not be decoded.
    #[error("failed to decode Opus packet")]
    DecodeFailed,
}

/// Encodes 10 ms PCM frames into Opus packets.
///
/// ```
/// use libwebrtc::{
///     audio_frame::AudioFrame,
///     native::audio_codec::{OpusEncoder, OpusEncoderConfig},
/// };
///
/// let mut encoder = OpusEncoder::new(&OpusEncoderConfig::default()).unwrap();
/// let silence = AudioFrame::new(48_000, 1, 480);
/// // The first 10 ms are buffered; the second completes a 20 ms packet.
/// assert!(encoder.encode(&silence).unwrap().is_none());
/// let packet = encoder.encode(&silence).unwrap().unwrap();
/// assert_eq!((packet.timestamp, packet.samples_per_channel), (0, 960));
/// ```
pub struct OpusEncoder {
    handle: UniquePtr<ac_sys::ffi::NativeOpusEncoder>,
    config: OpusEncoderConfig,
}

impl OpusEncoder {
    /// Creates an encoder.
    pub fn new(config: &OpusEncoderConfig) -> Result<Self, OpusCreateError> {
        let settings = ac_sys::ffi::OpusEncoderSettings {
            sample_rate: config.sample_rate,
            num_channels: config.num_channels,
            stereo: config.stereo,
            bitrate_bps: config.bitrate_bps,
            dtx: config.dtx,
            fec: config.fec,
            frame_duration_ms: config.frame_duration_ms,
        };
        let handle = ac_sys::ffi::create_opus_encoder(&settings)
            .map_err(|e| OpusCreateError { reason: e.what().to_owned() })?;
        Ok(Self { handle, config: config.clone() })
    }

    /// Encodes 10 ms of interleaved audio in the configured input format.
    ///
    /// Returns a packet once enough audio for one was encoded. With DTX, no
    /// packet may be produced for silent periods; the timestamps of later
    /// packets still advance.
    pub fn encode(
        &mut self,
        frame: &AudioFrame<'_>,
    ) -> Result<Option<OpusPacket>, OpusEncodeError> {
        let expected_samples = (self.config.sample_rate / 100) as usize;
        if frame.sample_rate != self.config.sample_rate
            || frame.num_channels != self.config.num_channels
            || frame.samples_per_channel as usize != expected_samples
            || frame.data.len() != expected_samples * self.config.num_channels as usize
        {
            return Err(OpusEncodeError::InvalidFrame);
        }

        let result = self.handle.pin_mut().encode(&frame.data);
        if !result.ok {
            return Err(OpusEncodeError::InvalidFrame);
        }
        Ok(result.has_packet.then(|| OpusPacket {
            data: self.handle.packet().to_vec(),
            audio_level: result.audio_level,
            timestamp: result.timestamp,
            samples_per_channel: result.samples_per_channel,
        }))
    }

    /// Changes the target bitrate.
    pub fn set_bitrate(&mut self, bitrate_bps: u32) {
        self.handle.pin_mut().set_bitrate(bitrate_bps);
    }

    /// Returns the current target bitrate.
    pub fn bitrate(&self) -> u32 {
        self.handle.bitrate()
    }

    /// Sets the expected packet loss rate (`0.0..=1.0`), which tunes in-band
    /// FEC.
    pub fn set_packet_loss_rate(&mut self, fraction: f32) {
        self.handle.pin_mut().set_packet_loss_rate(fraction.clamp(0.0, 1.0));
    }
}

/// Decodes Opus packets into PCM frames.
pub struct OpusDecoder {
    handle: UniquePtr<ac_sys::ffi::NativeOpusDecoder>,
    sample_rate: u32,
    num_channels: u32,
}

impl OpusDecoder {
    /// Creates a decoder producing interleaved audio at `sample_rate` (a
    /// multiple of 100 between 8 and 96 kHz) with `num_channels` (1 or 2).
    pub fn new(sample_rate: u32, num_channels: u32) -> Result<Self, OpusCreateError> {
        let handle = ac_sys::ffi::create_opus_decoder(sample_rate, num_channels)
            .map_err(|e| OpusCreateError { reason: e.what().to_owned() })?;
        Ok(Self { handle, sample_rate, num_channels })
    }

    /// Decodes one packet.
    pub fn decode(&mut self, packet: &[u8]) -> Result<AudioFrame<'static>, OpusDecodeError> {
        if packet.is_empty() {
            return Err(OpusDecodeError::DecodeFailed);
        }
        if !self.handle.pin_mut().decode(packet) {
            return Err(OpusDecodeError::DecodeFailed);
        }
        Ok(self.frame(self.handle.output().to_vec()))
    }

    /// Produces audio for one lost packet.
    ///
    /// When `next_packet` (the packet after the lost one) carries in-band
    /// FEC, the lost audio is recovered from it; decode `next_packet` itself
    /// afterwards as usual. Otherwise the audio is extrapolated from the
    /// previous packets (packet loss concealment), for the duration of the
    /// last decoded packet.
    pub fn conceal(
        &mut self,
        next_packet: Option<&[u8]>,
    ) -> Result<AudioFrame<'static>, OpusDecodeError> {
        if !self.handle.pin_mut().conceal(next_packet.unwrap_or_default()) {
            return Err(OpusDecodeError::DecodeFailed);
        }
        Ok(self.frame(self.handle.output().to_vec()))
    }

    fn frame(&self, pcm: Vec<i16>) -> AudioFrame<'static> {
        let samples_per_channel = (pcm.len() / self.num_channels as usize) as u32;
        AudioFrame {
            data: pcm.into(),
            sample_rate: self.sample_rate,
            num_channels: self.num_channels,
            samples_per_channel,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_packet_level_covers_its_pcm_samples() {
        let mut encoder = OpusEncoder::new(&OpusEncoderConfig::default()).unwrap();
        let frame = |value| AudioFrame {
            data: vec![value; 480].into(),
            sample_rate: 48_000,
            num_channels: 1,
            samples_per_channel: 480,
        };
        for (first, second, expected) in
            [(0, 0, 127), (16384, 16384, 6), (16384, 0, 9), (0, 0, 127)]
        {
            assert!(encoder.encode(&frame(first)).unwrap().is_none());
            let packet = encoder.encode(&frame(second)).unwrap().unwrap();
            assert_eq!(packet.audio_level & 0x7f, expected);
        }
    }
}
