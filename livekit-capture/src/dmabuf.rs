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

/// DMA-BUF pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaBufPixelFormat {
    /// NV12 biplanar format.
    Nv12,
    /// YUV420M multiplanar format.
    Yuv420M,
}

impl DmaBufPixelFormat {
    #[cfg(target_os = "linux")]
    pub(crate) fn as_native(self) -> i32 {
        match self {
            Self::Nv12 => 0,
            Self::Yuv420M => 1,
        }
    }
}

/// One DMA-BUF plane descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBufPlane {
    /// DMA-BUF file descriptor.
    pub fd: i32,
    /// Plane byte offset.
    pub offset: u32,
    /// Plane byte stride.
    pub stride: u32,
}

/// One DMA-BUF backed captured frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaBufFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Pixel format.
    pub pixel_format: DmaBufPixelFormat,
    /// DMA-BUF planes.
    pub planes: Vec<DmaBufPlane>,
    /// Optional DRM format modifier.
    pub modifier: Option<u64>,
    /// Capture timestamp in microseconds.
    pub timestamp_us: i64,
    /// Sensor timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
}
