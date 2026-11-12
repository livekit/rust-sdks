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

//! The part of the kernel's `videodev2.h` an encoder needs, for 64-bit targets.
//! Checked in rather than generated: the kernel keeps this ABI stable.

#![allow(non_camel_case_types)]

use std::mem::{offset_of, size_of};

use libc::{c_ulong, c_void, timeval};

pub(super) const VIDEO_MAX_PLANES: usize = 8;

pub(super) const V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE: u32 = 9;
pub(super) const V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE: u32 = 10;

pub(super) const V4L2_CAP_VIDEO_M2M_MPLANE: u32 = 0x0000_4000;
pub(super) const V4L2_CAP_STREAMING: u32 = 0x0400_0000;
pub(super) const V4L2_CAP_DEVICE_CAPS: u32 = 0x8000_0000;

pub(super) const V4L2_MEMORY_MMAP: u32 = 1;
pub(super) const V4L2_FIELD_NONE: u32 = 1;

pub(super) const V4L2_BUF_FLAG_KEYFRAME: u32 = 0x0000_0008;
pub(super) const V4L2_BUF_FLAG_ERROR: u32 = 0x0000_0040;

pub(super) const V4L2_CTRL_FLAG_DISABLED: u32 = 0x0001;
pub(super) const V4L2_SEL_TGT_CROP: u32 = 0;

#[repr(C)]
pub(super) struct v4l2_capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
pub(super) struct v4l2_fmtdesc {
    pub index: u32,
    pub type_: u32,
    pub flags: u32,
    pub description: [u8; 32],
    pub pixelformat: u32,
    pub mbus_code: u32,
    pub reserved: [u32; 3],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(super) struct v4l2_plane_pix_format {
    pub sizeimage: u32,
    pub bytesperline: u32,
    pub reserved: [u16; 6],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(super) struct v4l2_pix_format_mplane {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub colorspace: u32,
    pub plane_fmt: [v4l2_plane_pix_format; VIDEO_MAX_PLANES],
    pub num_planes: u8,
    pub flags: u8,
    pub ycbcr_enc: u8,
    pub quantization: u8,
    pub xfer_func: u8,
    pub reserved: [u8; 7],
}

/// `raw` gives the union the kernel's size and 8-byte alignment.
#[repr(C)]
pub(super) struct v4l2_format {
    pub type_: u32,
    pub fmt: v4l2_format_union,
}

#[repr(C)]
pub(super) union v4l2_format_union {
    pub pix_mp: v4l2_pix_format_mplane,
    pub raw: [u64; 25],
}

#[repr(C)]
pub(super) struct v4l2_requestbuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub flags: u8,
    pub reserved: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct v4l2_timecode {
    pub type_: u32,
    pub flags: u32,
    pub frames: u8,
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub userbits: [u8; 4],
}

/// Of the `m` union, only `mem_offset` is declared.
#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct v4l2_plane {
    pub bytesused: u32,
    pub length: u32,
    pub mem_offset: u32,
    pub m_padding: u32,
    pub data_offset: u32,
    pub reserved: [u32; 11],
}

/// `m` is the `planes` member of the kernel's union.
#[repr(C)]
pub(super) struct v4l2_buffer {
    pub index: u32,
    pub type_: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub field: u32,
    pub timestamp: timeval,
    pub timecode: v4l2_timecode,
    pub sequence: u32,
    pub memory: u32,
    pub m: *mut v4l2_plane,
    pub length: u32,
    pub reserved2: u32,
    pub request_fd: i32,
}

/// Of the value union, only the 32-bit integer is declared.
#[repr(C, packed)]
pub(super) struct v4l2_ext_control {
    pub id: u32,
    pub size: u32,
    pub reserved2: u32,
    pub value: i32,
    pub value_padding: u32,
}

#[repr(C)]
pub(super) struct v4l2_ext_controls {
    pub which: u32,
    pub count: u32,
    pub error_idx: u32,
    pub request_fd: i32,
    pub reserved: u32,
    pub controls: *mut v4l2_ext_control,
}

#[repr(C)]
pub(super) struct v4l2_queryctrl {
    pub id: u32,
    pub type_: u32,
    pub name: [u8; 32],
    pub minimum: i32,
    pub maximum: i32,
    pub step: i32,
    pub default_value: i32,
    pub flags: u32,
    pub reserved: [u32; 2],
}

/// The union's `v4l2_outputparm` member, inlined.
#[repr(C)]
pub(super) struct v4l2_streamparm {
    pub type_: u32,
    pub capability: u32,
    pub outputmode: u32,
    pub timeperframe_numerator: u32,
    pub timeperframe_denominator: u32,
    pub extendedmode: u32,
    pub writebuffers: u32,
    pub reserved: [u32; 44],
}

/// With `v4l2_rect` inlined.
#[repr(C)]
pub(super) struct v4l2_selection {
    pub type_: u32,
    pub target: u32,
    pub flags: u32,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub reserved: [u32; 9],
}

// `_IOC` from the kernel's `asm-generic/ioctl.h`.
const fn ioc(dir: c_ulong, nr: c_ulong, size: usize) -> c_ulong {
    (dir << 30) | ((size as c_ulong) << 16) | ((b'V' as c_ulong) << 8) | nr
}
const fn ior<T>(nr: c_ulong) -> c_ulong {
    ioc(2, nr, size_of::<T>())
}
const fn iow<T>(nr: c_ulong) -> c_ulong {
    ioc(1, nr, size_of::<T>())
}
const fn iowr<T>(nr: c_ulong) -> c_ulong {
    ioc(3, nr, size_of::<T>())
}

pub(crate) const VIDIOC_QUERYCAP: c_ulong = ior::<v4l2_capability>(0);
pub(crate) const VIDIOC_ENUM_FMT: c_ulong = iowr::<v4l2_fmtdesc>(2);
pub(crate) const VIDIOC_S_FMT: c_ulong = iowr::<v4l2_format>(5);
pub(crate) const VIDIOC_REQBUFS: c_ulong = iowr::<v4l2_requestbuffers>(8);
pub(crate) const VIDIOC_QUERYBUF: c_ulong = iowr::<v4l2_buffer>(9);
pub(crate) const VIDIOC_QBUF: c_ulong = iowr::<v4l2_buffer>(15);
pub(crate) const VIDIOC_DQBUF: c_ulong = iowr::<v4l2_buffer>(17);
pub(crate) const VIDIOC_STREAMON: c_ulong = iow::<i32>(18);
pub(crate) const VIDIOC_STREAMOFF: c_ulong = iow::<i32>(19);
pub(crate) const VIDIOC_S_PARM: c_ulong = iowr::<v4l2_streamparm>(22);
pub(crate) const VIDIOC_QUERYCTRL: c_ulong = iowr::<v4l2_queryctrl>(36);
pub(crate) const VIDIOC_S_EXT_CTRLS: c_ulong = iowr::<v4l2_ext_controls>(72);
pub(crate) const VIDIOC_S_SELECTION: c_ulong = iowr::<v4l2_selection>(95);

// The kernel's values on 64-bit targets.
const _: () = {
    assert!(size_of::<*mut c_void>() == 8);
    assert!(size_of::<timeval>() == 16);

    assert!(size_of::<v4l2_capability>() == 104);
    assert!(size_of::<v4l2_fmtdesc>() == 64);
    assert!(size_of::<v4l2_plane_pix_format>() == 20);
    assert!(size_of::<v4l2_pix_format_mplane>() == 192);
    assert!(offset_of!(v4l2_pix_format_mplane, plane_fmt) == 20);
    assert!(offset_of!(v4l2_pix_format_mplane, num_planes) == 180);
    assert!(size_of::<v4l2_format>() == 208);
    assert!(offset_of!(v4l2_format, fmt) == 8);
    assert!(size_of::<v4l2_requestbuffers>() == 20);
    assert!(size_of::<v4l2_plane>() == 64);
    assert!(offset_of!(v4l2_plane, mem_offset) == 8);
    assert!(offset_of!(v4l2_plane, data_offset) == 16);
    assert!(size_of::<v4l2_buffer>() == 88);
    assert!(offset_of!(v4l2_buffer, timestamp) == 24);
    assert!(offset_of!(v4l2_buffer, timecode) == 40);
    assert!(offset_of!(v4l2_buffer, sequence) == 56);
    assert!(offset_of!(v4l2_buffer, m) == 64);
    assert!(offset_of!(v4l2_buffer, length) == 72);
    assert!(offset_of!(v4l2_buffer, request_fd) == 80);
    assert!(size_of::<v4l2_ext_control>() == 20);
    assert!(offset_of!(v4l2_ext_control, value) == 12);
    assert!(size_of::<v4l2_ext_controls>() == 32);
    assert!(offset_of!(v4l2_ext_controls, controls) == 24);
    assert!(size_of::<v4l2_queryctrl>() == 68);
    assert!(size_of::<v4l2_streamparm>() == 204);
    assert!(size_of::<v4l2_selection>() == 64);

    assert!(VIDIOC_QUERYCAP == 0x8068_5600);
    assert!(VIDIOC_ENUM_FMT == 0xc040_5602);
    assert!(VIDIOC_S_FMT == 0xc0d0_5605);
    assert!(VIDIOC_REQBUFS == 0xc014_5608);
    assert!(VIDIOC_QUERYBUF == 0xc058_5609);
    assert!(VIDIOC_QBUF == 0xc058_560f);
    assert!(VIDIOC_DQBUF == 0xc058_5611);
    assert!(VIDIOC_STREAMON == 0x4004_5612);
    assert!(VIDIOC_STREAMOFF == 0x4004_5613);
    assert!(VIDIOC_S_PARM == 0xc0cc_5616);
    assert!(VIDIOC_QUERYCTRL == 0xc044_5624);
    assert!(VIDIOC_S_EXT_CTRLS == 0xc020_5648);
    assert!(VIDIOC_S_SELECTION == 0xc040_565f);
};
