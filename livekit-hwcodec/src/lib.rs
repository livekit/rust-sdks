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

//! **Internal crate.** Used by the LiveKit Rust SDK; not intended for direct use.
//! No API stability guarantees: any release may contain breaking changes.
//!
//! Hardware video encoders. Only V4L2 on 64-bit Linux so far; empty elsewhere.

#![deny(unsafe_code)]

/// One plane of an [`I420`] frame.
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

/// A borrowed I420 frame.
#[derive(Clone, Copy, Debug)]
pub struct I420<'a> {
    pub width: u32,
    pub height: u32,
    pub y: Plane<'a>,
    pub u: Plane<'a>,
    pub v: Plane<'a>,
}

/// A codec a hardware encoder can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Codec {
    H264,
}

/// What an encoder session produces.
#[derive(Clone, Copy, Debug)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub bitrate_bps: u32,
    pub framerate: u32,
}

/// Identifies a frame and its output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMeta {
    pub rtp_timestamp: u32,
    pub capture_time_ms: i64,
}

/// A raw frame, held by the encoder until its pixels are copied.
pub trait Frame: Send + 'static {
    /// Calls `f` with the frame as I420, or returns `None` if it can't be converted.
    fn with_i420<R>(&self, f: impl FnOnce(I420<'_>) -> R) -> Option<R>;
}

/// Receives an encoder's output, on the encoder's thread.
pub trait Sink: Send + 'static {
    /// Takes one encoded frame. `data` is valid only for the call.
    fn deliver(&self, data: &[u8], meta: &FrameMeta, keyframe: bool);

    /// Reports a frame that produced no output.
    fn dropped(&self, meta: &FrameMeta);
}

/// What the encoder did with a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeStatus {
    Queued,
    /// The encoder was full.
    Dropped,
    /// The encoder has failed for good.
    Failed,
}

// 32-bit targets lay out `v4l2_buffer` differently.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod v4l2;

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
pub use v4l2::{V4l2Encoder, V4l2EncoderFactory};
