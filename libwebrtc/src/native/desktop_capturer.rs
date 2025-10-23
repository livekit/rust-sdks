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
use webrtc_sys::desktop_capturer::{self as sys_dc, ffi::new_desktop_capturer};

#[derive(Copy, Clone, Debug)]
pub struct DesktopCapturerOptions {
    pub window_capturer: bool,
    pub include_cursor: bool,
    #[cfg(target_os = "macos")]
    pub allow_sck_system_picker: bool,
}

impl Default for DesktopCapturerOptions {
    fn default() -> Self {
        Self {
            window_capturer: false,
            include_cursor: false,
            #[cfg(target_os = "macos")]
            allow_sck_system_picker: true,
        }
    }
}

impl DesktopCapturerOptions {
    pub(crate) fn new() -> Self {
        Self { window_capturer: false, include_cursor: false, ..Default::default() }
    }

    pub(crate) fn with_cursor(mut self, include: bool) -> Self {
        self.include_cursor = include;
        self
    }

    pub(crate) fn with_window_capturer(mut self, window_capturer: bool) -> Self {
        self.window_capturer = window_capturer;
        self
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_sck_system_picker(mut self, allow_sck_system_picker: bool) -> Self {
        self.allow_sck_system_picker = allow_sck_system_picker;
        self
    }

    pub(crate) fn to_sys_handle(&self) -> sys_dc::ffi::DesktopCapturerOptions {
        let mut sys_handle = sys_dc::ffi::DesktopCapturerOptions {
            window_capturer: self.window_capturer,
            include_cursor: self.include_cursor,
            allow_sck_system_picker: false,
        };
        #[cfg(target_os = "macos")]
        {
            sys_handle.allow_sck_system_picker = self.allow_sck_system_picker;
        }
        sys_handle
    }
}

pub struct DesktopCapturer {
    pub(crate) sys_handle: UniquePtr<sys_dc::ffi::DesktopCapturer>,
}

impl DesktopCapturer {
    pub fn new<T>(callback: T, options: DesktopCapturerOptions) -> Option<Self>
    where
        T: Fn(CaptureResult, DesktopFrame) + Send + 'static,
    {
        let callback = DesktopCallback::new(callback);
        let callback_wrapper = sys_dc::DesktopCapturerCallbackWrapper::new(Box::new(callback));
        let sys_handle = new_desktop_capturer(Box::new(callback_wrapper), options.to_sys_handle());
        if sys_handle.is_null() {
            None
        } else {
            Some(Self { sys_handle })
        }
    }

    pub fn capture_frame(&self) {
        self.sys_handle.capture_frame();
    }

    pub fn start(&mut self) {
        let pin_handle = self.sys_handle.pin_mut();
        pin_handle.start();
    }

    pub fn select_source(&self, id: u64) -> bool {
        self.sys_handle.select_source(id)
    }

    pub fn get_source_list(&self) -> Vec<CaptureSource> {
        let mut sources = Vec::new();
        let source_list = self.sys_handle.get_source_list();
        for source in source_list.iter() {
            sources.push(CaptureSource { sys_handle: source.clone() });
        }
        sources
    }
}

pub struct DesktopFrame {
    pub(crate) sys_handle: UniquePtr<sys_dc::ffi::DesktopFrame>,
}

impl DesktopFrame {
    pub fn new(sys_handle: UniquePtr<sys_dc::ffi::DesktopFrame>) -> Self {
        Self { sys_handle }
    }

    pub fn width(&self) -> i32 {
        self.sys_handle.width()
    }

    pub fn height(&self) -> i32 {
        self.sys_handle.height()
    }

    pub fn stride(&self) -> u32 {
        self.sys_handle.stride() as u32
    }

    pub fn left(&self) -> i32 {
        self.sys_handle.left()
    }

    pub fn top(&self) -> i32 {
        self.sys_handle.top()
    }

    pub fn data(&self) -> &[u8] {
        let data = self.sys_handle.data();
        unsafe { std::slice::from_raw_parts(data, self.stride() as usize * self.height() as usize) }
    }
}

pub struct DesktopCallback<T: Fn(CaptureResult, DesktopFrame) + Send> {
    callback: T,
}

impl<T> DesktopCallback<T>
where
    T: Fn(CaptureResult, DesktopFrame) + Send,
{
    pub fn new(callback: T) -> Self {
        Self { callback }
    }
}

impl<T> sys_dc::DesktopCapturerCallback for DesktopCallback<T>
where
    T: Fn(CaptureResult, DesktopFrame) + Send,
{
    fn on_capture_result(
        &self,
        result: sys_dc::ffi::CaptureResult,
        frame: UniquePtr<sys_dc::ffi::DesktopFrame>,
    ) {
        (self.callback)(capture_result_from_sys(result), DesktopFrame::new(frame));
    }
}

#[derive(Clone)]
pub struct CaptureSource {
    pub(crate) sys_handle: sys_dc::ffi::Source,
}

impl CaptureSource {
    pub fn id(&self) -> u64 {
        self.sys_handle.id
    }

    pub fn title(&self) -> String {
        self.sys_handle.title.clone()
    }

    pub fn display_id(&self) -> i64 {
        self.sys_handle.display_id
    }
}

pub(crate) enum CaptureResult {
    Success,
    ErrorTemporary,
    ErrorPermanent,
}

fn capture_result_from_sys(result: sys_dc::ffi::CaptureResult) -> CaptureResult {
    match result {
        sys_dc::ffi::CaptureResult::Success => CaptureResult::Success,
        sys_dc::ffi::CaptureResult::ErrorTemporary => CaptureResult::ErrorTemporary,
        sys_dc::ffi::CaptureResult::ErrorPermanent => CaptureResult::ErrorPermanent,
        _ => CaptureResult::ErrorPermanent,
    }
}
