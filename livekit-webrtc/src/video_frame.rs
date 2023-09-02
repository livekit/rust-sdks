// Copyright 2023 LiveKit, Inc.
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
    alloc::{self, handle_alloc_error, Layout},
    fmt::Debug,
    sync::Arc,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoRotation {
    Rotation0 = 0,
    Rotation90 = 90,
    Rotation180 = 180,
    Rotation270 = 270,
}

// Supp
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoFormatType {
    Argb,
    // BGRA,
    Abgr,
    // RGBA,
}

/// Used by libwebrtc to recognize which type of buffer is being passed in
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VideoBufferType {
    Native, // Can be used for custom encoders
    I420,
    I420A,
    I422,
    I444,
    I010,
    Nv12,
}

pub trait VideoBuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn buffer_type(&self) -> VideoBufferType;
    fn to_i420(&self) -> I420Buffer;
    fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    );
}

#[derive(Clone, Debug)]
pub struct VideoFrame<T>
where
    T: VideoBuffer,
{
    pub rotation: VideoRotation,
    pub timestamp_us: i64, // When the frame was captured in microseconds
    pub buffer: Arc<T>,
}

struct AlignedBuffer {
    ptr: *mut u8,
    layout: Layout,
}

impl AlignedBuffer {
    pub fn new(size: usize, alignment: usize) -> Self {
        let layout = Layout::from_size_align(size, alignment).unwrap();
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        Self { ptr, layout }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.ptr, self.layout) }
    }
}

#[derive(Debug, Clone)]
enum DataSource {
    AlignedBuffer(AlignedBuffer),
    Vec(Vec<u8>),
    #[cfg(not(target_arch = "wasm32"))]
    Native(),
}

/// By default when using the new method, the buffer is aligned to 64 bytes.
/// This isn't a hard requirement but can give performance boost when using SIMD for yuv conversion.
#[derive(Debug)]
pub struct I420Buffer {
    data_ptr: *mut u8,
    layout: Layout,
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
}

// TODO(theomonnom): Create from Vec<u8> and Box<[u8]>
impl I420Buffer {
    pub const fn data_size(height: u32, stride_y: u32, stride_u: u32, stride_v: u32) -> usize {
        stride_y * height + (stride_u + stride_v) * ((height + 1) / 2)
    }

    pub fn new(width: u32, height: u32, stride_y: u32, stride_u: u32, stride_v: u32) -> Self {
        const OPTIMIZED_ALIGNMENT: usize = 64; // For faster libyuv conversions

        let layout = Layout::from_size_align(
            Self::data_size(height, stride_y, stride_u, stride_v),
            OPTIMIZED_ALIGNMENT,
        )
        .unwrap();

        let data_ptr = unsafe { alloc::alloc_zeroed(layout) };
        if data_ptr.is_null() {
            handle_alloc_error(layout);
        }

        Self {
            data_ptr,
            layout,
            width,
            height,
            stride_y,
            stride_u,
            stride_v,
        }
    }

    pub fn strides(&self) -> (u32, u32, u32) {
        (self.stride_y, self.stride_u, self.stride_v)
    }

    pub fn data(&self) -> (&[u8], &[u8], &[u8]) {
        unsafe {
            let ptr_y = self.ptr as *const u8;
            let ptr_u = ptr_y.add((self.stride_y * self.height) as usize);
            let ptr_v = ptr_u.add((self.stride_u * self.chroma_height()) as usize);
            (
                std::slice::from_raw_parts(ptr_y, (self.stride_y * self.height) as usize),
                std::slice::from_raw_parts(ptr_u, (self.stride_u * self.chroma_height()) as usize),
                std::slice::from_raw_parts(ptr_v, (self.stride_v * self.chroma_height()) as usize),
            )
        }
    }

    pub fn data_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let (data_y, data_u, data_v) = self.as_slice();
        unsafe {
            (
                std::slice::from_raw_parts_mut(data_y.as_ptr() as *mut u8, data_y.len()),
                std::slice::from_raw_parts_mut(data_u.as_ptr() as *mut u8, data_u.len()),
                std::slice::from_raw_parts_mut(data_v.as_ptr() as *mut u8, data_v.len()),
            )
        }
    }

    pub fn chroma_width(&self) -> u32 {
        (self.width + 1) / 2
    }

    pub fn chroma_height(&self) -> u32 {
        (self.height + 1) / 2
    }
}

impl VideoBuffer for I420Buffer {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn buffer_type(&self) -> VideoBufferType {
        VideoBufferType::I420
    }

    fn to_i420(&self) -> I420Buffer {
        self.clone()
    }

    fn to_argb(
        &self,
        format: VideoFormatType,
        dst: &mut [u8],
        dst_stride: u32,
        dst_width: i32,
        dst_height: i32,
    ) {
        // Convert to argb
    }
}

impl Clone for I420Buffer {
    fn clone(&self) -> Self {
        let mut new = Self::new(
            self.width,
            self.height,
            self.stride_y,
            self.stride_u,
            self.stride_v,
        );
        let (data_y, data_u, data_v) = self.as_slice();
        let (mut new_y, mut new_u, mut new_v) = new.as_slice_mut();
        new_y.copy_from_slice(data_y);
        new_u.copy_from_slice(data_u);
        new_v.copy_from_slice(data_v);
        new
    }
}

impl Drop for I420Buffer {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.ptr, self.layout) }
    }
}
