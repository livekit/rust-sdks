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

// 32-bit targets lay out `v4l2_buffer` differently.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod v4l2;
