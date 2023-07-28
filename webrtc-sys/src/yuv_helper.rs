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

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/yuv_helper.h");

        unsafe fn i420_to_argb(
            src_y: *const u8,
            src_stride_y: i32,
            src_u: *const u8,
            src_stride_u: i32,
            src_v: *const u8,
            src_stride_v: i32,
            dst_argb: *mut u8,
            dst_stride_argb: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn i420_to_bgra(
            src_y: *const u8,
            src_stride_y: i32,
            src_u: *const u8,
            src_stride_u: i32,
            src_v: *const u8,
            src_stride_v: i32,
            dst_bgra: *mut u8,
            dst_stride_bgra: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn i420_to_abgr(
            src_y: *const u8,
            src_stride_y: i32,
            src_u: *const u8,
            src_stride_u: i32,
            src_v: *const u8,
            src_stride_v: i32,
            dst_abgr: *mut u8,
            dst_stride_abgr: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn i420_to_rgba(
            src_y: *const u8,
            src_stride_y: i32,
            src_u: *const u8,
            src_stride_u: i32,
            src_v: *const u8,
            src_stride_v: i32,
            dst_rgba: *mut u8,
            dst_stride_rgba: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn argb_to_i420(
            src_argb: *const u8,
            src_stride_argb: i32,
            dst_y: *mut u8,
            dst_stride_y: i32,
            dst_u: *mut u8,
            dst_stride_u: i32,
            dst_v: *mut u8,
            dst_stride_v: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn abgr_to_i420(
            src_abgr: *const u8,
            src_stride_abgr: i32,
            dst_y: *mut u8,
            dst_stride_y: i32,
            dst_u: *mut u8,
            dst_stride_u: i32,
            dst_v: *mut u8,
            dst_stride_v: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;

        unsafe fn argb_to_rgb24(
            src_argb: *const u8,
            src_stride_argb: i32,
            dst_rgb24: *mut u8,
            dst_stride_rgb24: i32,
            width: i32,
            height: i32,
        ) -> Result<()>;
    }
}
