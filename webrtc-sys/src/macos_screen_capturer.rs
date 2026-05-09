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

use cxx::UniquePtr;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    /// A display available to ScreenCaptureKit.
    #[derive(Clone, Debug)]
    struct MacosScreen {
        id: u32,
        title: String,
        width: i32,
        height: i32,
    }

    /// Result code emitted by the ScreenCaptureKit callback.
    enum MacosScreenCaptureResult {
        Success,
        ErrorTemporary,
        ErrorPermanent,
    }

    unsafe extern "C++" {
        include!("livekit/macos_screen_capturer.h");

        /// A ScreenCaptureKit-backed screen capturer.
        type MacosScreenCapturer;
        /// A captured frame backed by a retained `CVPixelBufferRef`.
        type MacosScreenFrame;

        /// Creates a macOS ScreenCaptureKit capturer.
        fn new_macos_screen_capturer() -> UniquePtr<MacosScreenCapturer>;
        /// Returns the current list of capturable displays.
        fn get_screen_list(self: &MacosScreenCapturer) -> Vec<MacosScreen>;
        /// Starts streaming frames from a display.
        fn start(
            self: Pin<&mut MacosScreenCapturer>,
            display_id: u32,
            fps: u32,
            callback: Box<MacosScreenCapturerCallbackWrapper>,
        ) -> bool;
        /// Stops streaming frames.
        fn stop(self: Pin<&mut MacosScreenCapturer>);

        /// Returns the frame width in pixels.
        fn width(self: &MacosScreenFrame) -> i32;
        /// Returns the frame height in pixels.
        fn height(self: &MacosScreenFrame) -> i32;
        /// Returns a retained `CVPixelBufferRef` as an opaque pointer value.
        fn pixel_buffer(self: &MacosScreenFrame) -> usize;
    }

    extern "Rust" {
        type MacosScreenCapturerCallbackWrapper;

        fn on_capture_result(
            self: &mut MacosScreenCapturerCallbackWrapper,
            result: MacosScreenCaptureResult,
            frame: UniquePtr<MacosScreenFrame>,
        );
    }
}

impl_thread_safety!(ffi::MacosScreenCapturer, Send + Sync);
impl_thread_safety!(ffi::MacosScreenFrame, Send + Sync);

/// Error returned by macOS native screen capture.
#[derive(Debug, PartialEq)]
pub enum MacosScreenCaptureError {
    /// A temporary capture error occurred.
    Temporary,
    /// A permanent capture error occurred.
    Permanent,
}

/// Callback invoked by the macOS native screen capturer.
pub trait MacosScreenCapturerCallback: Send {
    /// Handles a captured frame or capture error.
    fn on_capture_result(
        &mut self,
        result: Result<UniquePtr<ffi::MacosScreenFrame>, MacosScreenCaptureError>,
    );
}

/// CXX bridge wrapper for a macOS screen capture callback.
pub struct MacosScreenCapturerCallbackWrapper {
    callback: Box<dyn MacosScreenCapturerCallback>,
}

impl MacosScreenCapturerCallbackWrapper {
    /// Creates a callback wrapper from a Rust callback.
    pub fn new(callback: Box<dyn MacosScreenCapturerCallback>) -> Self {
        Self { callback }
    }

    fn on_capture_result(
        &mut self,
        result: ffi::MacosScreenCaptureResult,
        frame: UniquePtr<ffi::MacosScreenFrame>,
    ) {
        match result {
            ffi::MacosScreenCaptureResult::Success => self.callback.on_capture_result(Ok(frame)),
            ffi::MacosScreenCaptureResult::ErrorTemporary => {
                self.callback.on_capture_result(Err(MacosScreenCaptureError::Temporary))
            }
            ffi::MacosScreenCaptureResult::ErrorPermanent => {
                self.callback.on_capture_result(Err(MacosScreenCaptureError::Permanent))
            }
            _ => self.callback.on_capture_result(Err(MacosScreenCaptureError::Permanent)),
        }
    }
}
