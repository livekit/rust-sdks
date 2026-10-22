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

//! The D3D11 native buffer (Livenalt fork patch 3). Needs the native libwebrtc
//! and a Direct3D 11 device; a machine without a GPU gets WARP.

#![cfg(target_os = "windows")]

#[path = "support/d3d11.rs"]
mod d3d;

use libwebrtc::video_frame::{
    native::{NativeBuffer, VideoFrameBufferExt},
    I420Buffer, VideoBuffer, VideoBufferType,
};
use libwebrtc::video_source::VideoResolution;
use windows::core::Interface;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const RED: [u8; 4] = [0, 0, 255, 255]; // BGRA

/// BT.601 limited range, as libyuv's ARGBToI420 writes it: white is
/// (235, 128, 128), pure red (82, 90, 240).
const WHITE_YUV: (u8, u8, u8) = (235, 128, 128);
const RED_YUV: (u8, u8, u8) = (82, 90, 240);

fn yuv_at(i420: &I420Buffer, x: u32, y: u32) -> (u8, u8, u8) {
    let (sy, su, sv) = i420.strides();
    let (py, pu, pv) = i420.data();
    (
        py[(y * sy + x) as usize],
        pu[(y / 2 * su + x / 2) as usize],
        pv[(y / 2 * sv + x / 2) as usize],
    )
}

fn assert_close(got: (u8, u8, u8), want: (u8, u8, u8), what: &str) {
    let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 3;
    assert!(
        near(got.0, want.0) && near(got.1, want.1) && near(got.2, want.2),
        "{what}: got {got:?}, want {want:?} (within 3)"
    );
}

/// The readback counter is process-wide, so the tests take turns.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn turn() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Left half white, right half red.
fn halves(width: u32) -> impl Fn(u32, u32) -> [u8; 4] {
    move |x, _| if x < width / 2 { WHITE } else { RED }
}

#[test]
fn a_bgra_texture_becomes_a_native_buffer_that_converts_to_i420() {
    let _turn = turn();
    let device = d3d::Device::new();
    let texture = device.texture(64, 32, halves(64));

    let buffer = unsafe {
        NativeBuffer::from_d3d11_texture(
            texture.as_raw(),
            VideoResolution { width: 64, height: 32 },
            (),
        )
    }
    .expect("a BGRA texture is accepted");

    assert_eq!(buffer.buffer_type(), VideoBufferType::Native);
    assert_eq!((buffer.width(), buffer.height()), (64, 32));
    assert_eq!(buffer.get_d3d11_texture(), Some(texture.as_raw()));

    let i420 = buffer.to_i420();
    assert_eq!((i420.width(), i420.height()), (64, 32));
    assert_close(yuv_at(&i420, 8, 8), WHITE_YUV, "left half");
    assert_close(yuv_at(&i420, 56, 24), RED_YUV, "right half");
}

fn wrap(
    texture: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
    w: u32,
    h: u32,
) -> NativeBuffer {
    unsafe {
        NativeBuffer::from_d3d11_texture(
            texture.as_raw(),
            VideoResolution { width: w, height: h },
            (),
        )
    }
    .expect("a BGRA texture is accepted")
}

#[test]
fn a_frame_is_read_back_once_however_often_it_is_mapped() {
    let _turn = turn();
    let device = d3d::Device::new();
    let texture = device.texture(64, 32, halves(64));
    let buffer = wrap(&texture, 64, 32);

    let before = libwebrtc::video_frame::native::d3d11_readback_count();
    let first = buffer.to_i420();
    let second = buffer.to_i420();
    assert_eq!(libwebrtc::video_frame::native::d3d11_readback_count() - before, 1);
    assert_eq!(first.data().0[..64], second.data().0[..64]);

    // A second frame on the same texture is a new readback.
    let again = wrap(&texture, 64, 32);
    again.to_i420();
    assert_eq!(libwebrtc::video_frame::native::d3d11_readback_count() - before, 2);
}

#[test]
fn a_texture_the_video_processor_cannot_take_converts_on_the_cpu() {
    let _turn = turn();
    let device = d3d::Device::new();
    let texture = device.staging_texture(64, 32, halves(64));
    let buffer = wrap(&texture, 64, 32);

    let before = libwebrtc::video_frame::native::d3d11_readback_count();
    let i420 = buffer.to_i420();
    assert_eq!(libwebrtc::video_frame::native::d3d11_readback_count() - before, 1);
    assert_close(yuv_at(&i420, 8, 8), WHITE_YUV, "left half");
    assert_close(yuv_at(&i420, 56, 24), RED_YUV, "right half");
}

#[test]
fn only_the_requested_top_left_of_the_texture_is_shown() {
    let _turn = turn();
    let device = d3d::Device::new();
    // 64x32 texture, the frame is its left 30x20: all white.
    let texture = device.texture(64, 32, halves(64));
    let buffer = wrap(&texture, 30, 20);

    assert_eq!((buffer.width(), buffer.height()), (30, 20));
    let i420 = buffer.to_i420();
    assert_eq!((i420.width(), i420.height()), (30, 20));
    assert_close(yuv_at(&i420, 29, 19), WHITE_YUV, "bottom-right corner");
}

#[test]
fn textures_the_buffer_cannot_show_are_refused() {
    let _turn = turn();
    let device = d3d::Device::new();
    let texture = device.texture(64, 32, halves(64));
    let too_big = unsafe {
        NativeBuffer::from_d3d11_texture(
            texture.as_raw(),
            VideoResolution { width: 65, height: 32 },
            (),
        )
    };
    assert!(too_big.is_none());
    let null = unsafe {
        NativeBuffer::from_d3d11_texture(
            std::ptr::null_mut(),
            VideoResolution { width: 2, height: 2 },
            (),
        )
    };
    assert!(null.is_none());
}

#[test]
fn the_keep_alive_is_dropped_with_the_last_reference_to_the_frame() {
    let _turn = turn();
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Slot(Arc<AtomicBool>);
    impl Drop for Slot {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let device = d3d::Device::new();
    let texture = device.texture(16, 16, |_, _| WHITE);
    let released = Arc::new(AtomicBool::new(false));
    let buffer = unsafe {
        NativeBuffer::from_d3d11_texture(
            texture.as_raw(),
            VideoResolution { width: 16, height: 16 },
            Slot(released.clone()),
        )
    }
    .unwrap();

    buffer.to_i420();
    assert!(!released.load(Ordering::SeqCst), "released while the buffer is alive");
    drop(buffer);
    assert!(released.load(Ordering::SeqCst), "not released with the buffer");
}
