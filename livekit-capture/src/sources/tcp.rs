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

use std::{
    io::{self, Read},
    net::TcpStream,
};

use thiserror::Error;

use crate::{
    encoded::{
        h26x::AnnexBAccessUnitParser,
        ingress::EncodedAccessUnitSource,
        rtp::{RtpAccessUnitAssembler, RtpDepacketizerError},
        EncodedVideoCodec, EncodedWireFormat, OwnedEncodedAccessUnit,
    },
    error::CaptureError,
};

const DEFAULT_CHUNK_SIZE: usize = 4096;

/// Configuration for a byte-stream encoded source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteStreamSourceConfig {
    /// Declared stream wire format.
    pub wire_format: EncodedWireFormat,
    /// Timestamp assigned to the first emitted access unit.
    pub start_timestamp_us: i64,
    /// Frame interval used for Annex-B byte streams.
    pub frame_interval_us: i64,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Read chunk size for Annex-B byte streams.
    pub read_chunk_size: usize,
}

impl ByteStreamSourceConfig {
    /// Creates byte-stream source configuration with a 4096-byte read chunk.
    pub fn new(
        wire_format: EncodedWireFormat,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            wire_format,
            start_timestamp_us,
            frame_interval_us,
            width,
            height,
            read_chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Encoded source backed by any blocking byte stream.
#[derive(Debug)]
pub struct ByteStreamEncodedSource<R> {
    reader: R,
    parser: ByteStreamParser,
    read_chunk: Vec<u8>,
    eof: bool,
}

/// TCP encoded source using the same parser as other byte streams.
pub type TcpEncodedSource = ByteStreamEncodedSource<TcpStream>;

#[derive(Debug)]
enum ByteStreamParser {
    H26x(AnnexBAccessUnitParser),
    Rtp(RtpAccessUnitAssembler),
}

impl<R> ByteStreamEncodedSource<R>
where
    R: Read,
{
    /// Creates an encoded source for a declared byte-stream wire format.
    pub fn new(reader: R, config: ByteStreamSourceConfig) -> Result<Self, TcpSourceError> {
        let parser = match config.wire_format {
            EncodedWireFormat::H264AnnexB => ByteStreamParser::H26x(
                AnnexBAccessUnitParser::new(
                    EncodedVideoCodec::H264,
                    config.start_timestamp_us,
                    config.frame_interval_us,
                    config.width,
                    config.height,
                )
                .map_err(TcpSourceError::Capture)?,
            ),
            EncodedWireFormat::H265AnnexB => ByteStreamParser::H26x(
                AnnexBAccessUnitParser::new(
                    EncodedVideoCodec::H265,
                    config.start_timestamp_us,
                    config.frame_interval_us,
                    config.width,
                    config.height,
                )
                .map_err(TcpSourceError::Capture)?,
            ),
            EncodedWireFormat::Rtp { codec, clock_rate } => {
                ByteStreamParser::Rtp(RtpAccessUnitAssembler::new(
                    codec,
                    clock_rate,
                    config.start_timestamp_us,
                    config.width,
                    config.height,
                )?)
            }
            EncodedWireFormat::MpegTs => {
                return Err(TcpSourceError::UnsupportedWireFormat(config.wire_format));
            }
        };

        Ok(Self { reader, parser, read_chunk: vec![0; config.read_chunk_size.max(1)], eof: false })
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

    fn next_annex_b(
        reader: &mut R,
        read_chunk: &mut [u8],
        parser: &mut AnnexBAccessUnitParser,
        eof: &mut bool,
    ) -> Result<Option<OwnedEncodedAccessUnit>, TcpSourceError> {
        loop {
            if let Some(access_unit) = parser.push(&[]).map_err(TcpSourceError::Capture)? {
                return Ok(Some(access_unit));
            }
            if *eof {
                return parser.flush().map_err(TcpSourceError::Capture);
            }

            let read = reader.read(read_chunk).map_err(TcpSourceError::Io)?;
            if read == 0 {
                *eof = true;
                continue;
            }
            if let Some(access_unit) =
                parser.push(&read_chunk[..read]).map_err(TcpSourceError::Capture)?
            {
                return Ok(Some(access_unit));
            }
        }
    }

    fn next_rtp(
        reader: &mut R,
        assembler: &mut RtpAccessUnitAssembler,
        eof: &mut bool,
    ) -> Result<Option<OwnedEncodedAccessUnit>, TcpSourceError> {
        while !*eof {
            let mut len = [0u8; 2];
            if !read_exact_or_clean_eof(reader, &mut len).map_err(TcpSourceError::Io)? {
                *eof = true;
                return Ok(None);
            }

            let packet_len = u16::from_be_bytes(len) as usize;
            if packet_len == 0 {
                continue;
            }

            let mut packet = vec![0; packet_len];
            reader.read_exact(&mut packet).map_err(TcpSourceError::Io)?;
            if let Some(access_unit) = assembler.push(&packet)? {
                return Ok(Some(access_unit));
            }
        }

        Ok(None)
    }
}

impl<R> EncodedAccessUnitSource for ByteStreamEncodedSource<R>
where
    R: Read + Send + Sync + 'static,
{
    type Error = TcpSourceError;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        match &mut self.parser {
            ByteStreamParser::H26x(parser) => {
                Self::next_annex_b(&mut self.reader, &mut self.read_chunk, parser, &mut self.eof)
            }
            ByteStreamParser::Rtp(assembler) => {
                Self::next_rtp(&mut self.reader, assembler, &mut self.eof)
            }
        }
    }
}

/// Error returned by byte-stream encoded sources.
#[derive(Debug, Error)]
pub enum TcpSourceError {
    /// I/O failed while reading the byte stream.
    #[error("byte-stream read failed: {0}")]
    Io(io::Error),
    /// The declared wire format is not supported by this source.
    #[error("unsupported byte-stream wire format: {0:?}")]
    UnsupportedWireFormat(EncodedWireFormat),
    /// RTP depayloading failed.
    #[error(transparent)]
    Rtp(#[from] RtpDepacketizerError),
    /// Access-unit construction failed.
    #[error(transparent)]
    Capture(CaptureError),
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

    fn rfc4571(packet: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + packet.len());
        bytes.extend_from_slice(&(packet.len() as u16).to_be_bytes());
        bytes.extend_from_slice(packet);
        bytes
    }

    #[test]
    fn reads_annex_b_access_units() {
        let stream =
            [0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x65, 1, 2, 0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x41, 3];
        let config =
            ByteStreamSourceConfig::new(EncodedWireFormat::H264AnnexB, 0, 33_333, 640, 480);
        let mut source = ByteStreamEncodedSource::new(Cursor::new(stream), config).unwrap();

        let first = source.next_access_unit().unwrap().unwrap();
        assert_eq!(first.payload.as_ref(), &[0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x65, 1, 2]);
        let second = source.next_access_unit().unwrap().unwrap();
        assert_eq!(second.payload.as_ref(), &[0, 0, 1, 0x09, 0x10, 0, 0, 1, 0x41, 3]);
        assert!(source.next_access_unit().unwrap().is_none());
    }

    #[test]
    fn reads_rfc4571_rtp_access_unit() {
        let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
        let stream = rfc4571(&packet);
        let config = ByteStreamSourceConfig::new(
            EncodedWireFormat::Rtp { codec: EncodedVideoCodec::H264, clock_rate: 90_000 },
            0,
            33_333,
            640,
            480,
        );
        let mut source = ByteStreamEncodedSource::new(Cursor::new(stream), config).unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        assert!(source.next_access_unit().unwrap().is_none());
    }
}
