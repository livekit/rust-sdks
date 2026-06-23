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

use std::io::{self, Read};

use thiserror::Error;

use crate::encoded::{
    ingress::EncodedAccessUnitSource,
    rtp::{RtpAccessUnitAssembler, RtpDepacketizerError},
    EncodedVideoCodec, OwnedEncodedAccessUnit,
};

/// Configuration for RTSP interleaved RTP media.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtspInterleavedSourceConfig {
    /// RTP payload codec.
    pub codec: EncodedVideoCodec,
    /// RTP timestamp clock rate.
    pub clock_rate: u32,
    /// RTSP interleaved channel carrying video RTP packets.
    pub video_channel: u8,
    /// Timestamp assigned to the first emitted access unit.
    pub start_timestamp_us: i64,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
}

/// Encoded source for RTSP interleaved RTP streams.
#[derive(Debug)]
pub struct RtspInterleavedRtpSource<R> {
    reader: R,
    config: RtspInterleavedSourceConfig,
    assembler: RtpAccessUnitAssembler,
    eof: bool,
}

impl<R> RtspInterleavedRtpSource<R>
where
    R: Read,
{
    /// Creates a source for an RTSP stream that is already in interleaved RTP mode.
    pub fn new(reader: R, config: RtspInterleavedSourceConfig) -> Result<Self, RtspSourceError> {
        let assembler = RtpAccessUnitAssembler::new(
            config.codec,
            config.clock_rate,
            config.start_timestamp_us,
            config.width,
            config.height,
        )?;
        Ok(Self { reader, config, assembler, eof: false })
    }

    /// Returns the wrapped reader.
    pub fn reader(&self) -> &R {
        &self.reader
    }

    /// Returns the wrapped reader mutably.
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Consumes this source and returns its reader.
    pub fn into_reader(self) -> R {
        self.reader
    }

    fn read_next_interleaved_frame(&mut self) -> Result<Option<(u8, Vec<u8>)>, RtspSourceError> {
        while !self.eof {
            let mut magic = [0u8; 1];
            if !read_exact_or_clean_eof(&mut self.reader, &mut magic)
                .map_err(RtspSourceError::Io)?
            {
                self.eof = true;
                return Ok(None);
            }

            if magic[0] != b'$' {
                return Err(RtspSourceError::UnexpectedData);
            }

            let mut header = [0u8; 3];
            self.reader.read_exact(&mut header).map_err(RtspSourceError::Io)?;
            let channel = header[0];
            let len = u16::from_be_bytes([header[1], header[2]]) as usize;
            let mut payload = vec![0; len];
            self.reader.read_exact(&mut payload).map_err(RtspSourceError::Io)?;
            return Ok(Some((channel, payload)));
        }

        Ok(None)
    }
}

impl<R> EncodedAccessUnitSource for RtspInterleavedRtpSource<R>
where
    R: Read + Send + Sync + 'static,
{
    type Error = RtspSourceError;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        loop {
            let Some((channel, payload)) = self.read_next_interleaved_frame()? else {
                return Ok(None);
            };
            if channel != self.config.video_channel {
                continue;
            }
            if let Some(access_unit) = self.assembler.push(&payload)? {
                return Ok(Some(access_unit));
            }
        }
    }
}

/// Error returned by RTSP encoded sources.
#[derive(Debug, Error)]
pub enum RtspSourceError {
    /// I/O failed while reading RTSP interleaved data.
    #[error("RTSP read failed: {0}")]
    Io(io::Error),
    /// Interleaved RTP was malformed or a non-interleaved byte was encountered.
    #[error("unexpected RTSP interleaved data")]
    UnexpectedData,
    /// RTP depayloading failed.
    #[error(transparent)]
    Rtp(#[from] RtpDepacketizerError),
}

fn read_exact_or_clean_eof(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut offset = 0;
    while offset < buf.len() {
        match reader.read(&mut buf[offset..])? {
            0 if offset == 0 => return Ok(false),
            0 => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            read => offset += read,
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn rtp_packet(sequence_number: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.push(0x80);
        packet.push(if marker { 0x80 | 96 } else { 96 });
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    fn interleaved(channel: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.push(b'$');
        frame.push(channel);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn reads_rtsp_interleaved_rtp_access_unit() {
        let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
        let stream = interleaved(0, &packet);
        let config = RtspInterleavedSourceConfig {
            codec: EncodedVideoCodec::H264,
            clock_rate: 90_000,
            video_channel: 0,
            start_timestamp_us: 0,
            width: 640,
            height: 480,
        };
        let mut source = RtspInterleavedRtpSource::new(Cursor::new(stream), config).unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        assert!(source.next_access_unit().unwrap().is_none());
    }
}
