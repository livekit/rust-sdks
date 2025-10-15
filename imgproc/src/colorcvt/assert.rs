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

#[inline]
pub fn valid_420(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    width: u32,
    height: u32,
) {
    assert!(width > 0);
    assert!(height > 0);

    let chroma_width = (width + 1) / 2;
    let chroma_height = (height + 1) / 2;

    assert!(src_stride_y >= width);
    assert!(src_stride_u >= chroma_width);
    assert!(src_stride_v >= chroma_width);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_u.len() >= (src_stride_u * chroma_height) as usize);
    assert!(src_v.len() >= (src_stride_v * chroma_height) as usize);
}

#[inline]
pub fn valid_420a(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    src_a: &[u8],
    src_stride_a: u32,
    width: u32,
    height: u32,
) {
    assert!(width > 0);
    assert!(height > 0);

    let chroma_width = (width + 1) / 2;
    let chroma_height = (height + 1) / 2;

    assert!(src_stride_y >= width);
    assert!(src_stride_u >= chroma_width);
    assert!(src_stride_v >= chroma_width);
    assert!(src_stride_a >= width);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_u.len() >= (src_stride_u * chroma_height) as usize);
    assert!(src_v.len() >= (src_stride_v * chroma_height) as usize);
    assert!(src_a.len() >= (src_stride_a * height) as usize);
}

#[inline]
pub fn valid_422(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    width: u32,
    height: u32,
) {
    assert!(width > 0);
    assert!(height > 0);

    let chroma_width = (width + 1) / 2;
    let chroma_height = height;

    assert!(src_stride_y >= width);
    assert!(src_stride_u >= chroma_width);
    assert!(src_stride_v >= chroma_width);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_u.len() >= (src_stride_u * chroma_height) as usize);
    assert!(src_v.len() >= (src_stride_v * chroma_height) as usize);
}

#[inline]
pub fn valid_444(
    src_y: &[u8],
    src_stride_y: u32,
    src_u: &[u8],
    src_stride_u: u32,
    src_v: &[u8],
    src_stride_v: u32,
    width: u32,
    height: u32,
) {
    assert!(height > 0);
    assert!(width > 0);

    let chroma_width = width;
    let chroma_height = height;

    assert!(src_stride_y >= width);
    assert!(src_stride_u >= chroma_width);
    assert!(src_stride_v >= chroma_width);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_u.len() >= (src_stride_u * chroma_height) as usize);
    assert!(src_v.len() >= (src_stride_v * chroma_height) as usize);
}

#[inline]
pub fn valid_010(
    src_y: &[u16],
    src_stride_y: u32,
    src_u: &[u16],
    src_stride_u: u32,
    src_v: &[u16],
    src_stride_v: u32,
    width: u32,
    height: u32,
) {
    assert!(height > 0);
    assert!(width > 0);

    let chroma_width = (width + 1) / 2;
    let chroma_height = (height + 1) / 2;

    assert!(src_stride_y >= width);
    assert!(src_stride_u >= chroma_width);
    assert!(src_stride_v >= chroma_width);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_u.len() >= (src_stride_u * chroma_height) as usize);
    assert!(src_v.len() >= (src_stride_v * chroma_height) as usize);
}

#[inline]
pub fn valid_nv12(
    src_y: &[u8],
    src_stride_y: u32,
    src_uv: &[u8],
    src_stride_uv: u32,
    width: u32,
    height: u32,
) {
    assert!(width > 0);
    assert!(height > 0);

    let chroma_height = (height + 1) / 2;

    assert!(src_stride_y >= width);
    assert!(src_stride_uv >= width + width % 2);
    assert!(src_y.len() >= (src_stride_y * height) as usize);
    assert!(src_uv.len() >= (src_stride_uv * chroma_height) as usize);
}

#[inline]
pub fn valid_rgba(src_rgba: &[u8], src_stride_rgba: u32, width: u32, height: u32) {
    assert!(width > 0);
    assert!(height > 0);
    assert!(src_stride_rgba >= width * 4);
    assert!(src_rgba.len() >= (src_stride_rgba * height) as usize);
}

#[inline]
pub fn valid_rgb(src_rgb: &[u8], src_stride_rgb: u32, width: u32, height: u32) {
    assert!(width > 0);
    assert!(height > 0);
    assert!(src_stride_rgb >= width * 3);
    assert!(src_rgb.len() >= (src_stride_rgb * height) as usize);
}
