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

//! Pluggable encoded-frame ingest sources.
//!
//! The example wires a [`TcpH264Source`] into the LiveKit publish loop, but
//! the [`EncodedFrameSource`] trait is intentionally narrow so additional
//! transports (file, named pipe, gRPC, etc.) can be dropped in by
//! implementing the trait and swapping the constructor in `main.rs`.

use std::{future::Future, pin::Pin};

use anyhow::Result;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    net::TcpStream,
};

use crate::h264::{AnnexBParser, FrameAssembler, H264Frame};

/// Single encoded video frame produced by an ingest source.
#[derive(Debug)]
pub struct EncodedFrame {
    /// Concatenated Annex-B NALUs that make up the frame.
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub has_parameter_sets: bool,
}

impl From<H264Frame> for EncodedFrame {
    fn from(f: H264Frame) -> Self {
        Self {
            data: f.data,
            is_keyframe: f.is_keyframe,
            has_parameter_sets: f.has_parameter_sets,
        }
    }
}

/// Async source of pre-encoded video frames.
///
/// Implementors return one frame per `next_frame` call, or `Ok(None)` on
/// end-of-stream.  The trait is dyn-compatible so callers can hold a
/// `Box<dyn EncodedFrameSource>` and swap implementations at runtime.
pub trait EncodedFrameSource: Send {
    fn next_frame<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<EncodedFrame>>> + Send + 'a>>;
}

/// Reads an Annex-B H.264 elementary stream from a TCP server (e.g.
/// `gst-launch-1.0 ... ! tcpserversink host=0.0.0.0 port=5000`).
///
/// We are the *client*: gstreamer's `tcpserversink` listens for incoming
/// connections, so we open the socket with [`TcpStream::connect`] rather
/// than binding a listener of our own.
pub struct TcpH264Source {
    stream: Box<dyn AsyncRead + Send + Unpin>,
    parser: AnnexBParser,
    assembler: FrameAssembler,
    buf: Vec<u8>,
    pending: std::collections::VecDeque<H264Frame>,
    /// Total frames the assembler dropped while waiting for the first
    /// keyframe (informational).
    pub dropped_before_first_keyframe: u64,
    /// Total bytes pulled off the wire (informational).
    pub bytes_received: u64,
    eof: bool,
}

impl TcpH264Source {
    /// Connect to the given `host:port` and start reading bytes.
    pub async fn connect(addr: &str) -> Result<Self> {
        log::info!("Connecting to TCP server at {}...", addr);
        let stream = TcpStream::connect(addr).await?;
        log::info!("Connected to TCP server at {}", addr);
        Ok(Self::with_reader(Box::new(stream)))
    }

    /// Build a source from any `AsyncRead`.  Useful for tests or to plug in
    /// a file / pipe / Unix socket.
    pub fn with_reader(stream: Box<dyn AsyncRead + Send + Unpin>) -> Self {
        Self {
            stream,
            parser: AnnexBParser::new(),
            assembler: FrameAssembler::new(),
            buf: vec![0u8; 64 * 1024],
            pending: std::collections::VecDeque::new(),
            dropped_before_first_keyframe: 0,
            bytes_received: 0,
            eof: false,
        }
    }

    async fn next_frame_inner(&mut self) -> Result<Option<EncodedFrame>> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Ok(Some(frame.into()));
            }
            if self.eof {
                return Ok(self.assembler.flush_remaining().map(Into::into));
            }

            let n = self.stream.read(&mut self.buf).await?;
            if n == 0 {
                self.eof = true;
                continue;
            }
            self.bytes_received += n as u64;
            let nalus = self.parser.push(&self.buf[..n]);
            let (frames, dropped) = self.assembler.push_nalus(nalus);
            self.dropped_before_first_keyframe += dropped;
            for frame in frames {
                self.pending.push_back(frame);
            }
        }
    }
}

impl EncodedFrameSource for TcpH264Source {
    fn next_frame<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<EncodedFrame>>> + Send + 'a>> {
        Box::pin(self.next_frame_inner())
    }
}
