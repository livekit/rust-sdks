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

use crate::{impl_thread_safety, sys};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DesktopCaptureSourceType {
    Screen,
    Window,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Generic,
}

impl From<DesktopCaptureSourceType> for sys::lkSourceType {
    fn from(t: DesktopCaptureSourceType) -> Self {
        match t {
            DesktopCaptureSourceType::Screen => Self::SOURCE_TYPE_SCREEN,
            DesktopCaptureSourceType::Window => Self::SOURCE_TYPE_WINDOW,
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            DesktopCaptureSourceType::Generic => Self::SOURCE_TYPE_GENERIC,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum CaptureError {
    Temporary,
    Permanent,
}

pub enum CaptureResult {
    Success,
    ErrorTemporary,
    ErrorPermanent,
}

impl From<CaptureResult> for sys::lkCaptureResult {
    fn from(t: CaptureResult) -> Self {
        match t {
            CaptureResult::Success => Self::CAPTURE_RESULT_SUCCESS,
            CaptureResult::ErrorTemporary => Self::CAPTURE_RESULT_ERROR_TEMPORARY,
            CaptureResult::ErrorPermanent => Self::CAPTURE_RESULT_ERROR_PERMANENT,
        }
    }
}

pub struct DesktopFrame {
    pub ffi: sys::RefCounted<sys::lkDesktopFrame>,
}

impl DesktopFrame {
    pub fn new(ffi: sys::RefCounted<sys::lkDesktopFrame>) -> Self {
        Self { ffi }
    }

    pub fn width(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetWidth(self.ffi.as_ptr()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetHeight(self.ffi.as_ptr()) }
    }

    pub fn stride(&self) -> u32 {
        unsafe { sys::lkDesktopFrameGetStride(self.ffi.as_ptr()) }
    }

    pub fn left(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetLeft(self.ffi.as_ptr()) }
    }

    pub fn top(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetTop(self.ffi.as_ptr()) }
    }

    pub fn data(&self) -> &[u8] {
        unsafe {
            let lk_data =
                sys::RefCountedData::from_native(sys::lkDesktopFrameGetData(self.ffi.as_ptr()));
            std::slice::from_raw_parts(
                lk_data.as_bytes().as_ptr(),
                self.stride() as usize * self.height() as usize,
            )
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DesktopCapturerOptions {
    source_type: DesktopCaptureSourceType,
    include_cursor: bool,
    #[cfg(target_os = "macos")]
    allow_sck_system_picker: bool,
}

impl Default for DesktopCapturerOptions {
    fn default() -> Self {
        Self {
            source_type: DesktopCaptureSourceType::Screen,
            include_cursor: false,
            #[cfg(target_os = "macos")]
            allow_sck_system_picker: true,
        }
    }
}

impl DesktopCapturerOptions {
    pub fn new(source_type: DesktopCaptureSourceType) -> Self {
        Self { source_type, ..Default::default() }
    }

    pub fn set_include_cursor(mut self, include: bool) -> Self {
        self.include_cursor = include;
        self
    }

    #[cfg(target_os = "macos")]
    pub fn set_sck_system_picker(mut self, allow_sck_system_picker: bool) -> Self {
        self.allow_sck_system_picker = allow_sck_system_picker;
        self
    }

    pub fn to_sys_options(&self) -> sys::lkDesktopCapturerOptions {
        let mut sys_options = sys::lkDesktopCapturerOptions {
            source_type: self.source_type.into(),
            include_cursor: self.include_cursor,
            allow_sck_system_picker: false,
        };
        #[cfg(target_os = "macos")]
        {
            sys_options.allow_sck_system_picker = self.allow_sck_system_picker;
        }
        sys_options
    }
}

pub struct DesktopCapturer {
    ffi: sys::RefCounted<sys::lkDesktopCapturer>,
    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "glib-main-loop"))]
    glib_loop: Option<glib::MainLoop>,
}

impl DesktopCapturer {
    pub fn new(options: DesktopCapturerOptions) -> Option<Self> {
        unsafe {
            let ffi = sys::lkCreateDesktopCapturer(&options.to_sys_options());
            if ffi.is_null() {
                None
            } else {
                Some(Self {
                    ffi: sys::RefCounted::from_raw(ffi),
                    #[cfg(all(
                        any(target_os = "linux", target_os = "freebsd"),
                        feature = "glib-main-loop"
                    ))]
                    glib_loop: None,
                })
            }
        }
    }

    pub fn capture_frame(&self) {
        unsafe {
            sys::lkDesktopCapturerCaptureFrame(self.ffi.as_ptr());
        }
    }

    pub fn start_capture<T>(&mut self, source: Option<CaptureSource>, callback: T)
    where
        T: FnMut(Result<DesktopFrame, CaptureError>) + Send + 'static,
    {
        #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "glib-main-loop"))]
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let main_loop = glib::MainLoop::new(None, false);
            self.glib_loop = Some(main_loop.clone());
            let _handle = std::thread::spawn(move || {
                main_loop.run();
            });
        }
        let callback = DesktopCallback::new(callback);
        let callback_wrapper = DesktopCapturerCallbackWrapper::new(Box::new(callback));
        let callback_ptr = Box::into_raw(Box::new(callback_wrapper));
        unsafe {
            sys::lkDesktopCapturerStart(
                self.ffi.as_ptr(),
                Some(DesktopCapturerCallbackWrapper::on_capture_result),
                callback_ptr as *mut ::std::os::raw::c_void,
            );
        }
    }

    pub fn select_source(&self, id: u64) -> bool {
        unsafe { sys::lkDesktopCapturerSelectSource(self.ffi.as_ptr(), id) }
    }

    pub fn get_source_list(&self) -> Vec<CaptureSource> {
        let mut sources = Vec::new();
        let lk_vec = unsafe { sys::lkDesktopCapturerGetSourceList(self.ffi.as_ptr()) };
        let source_list = crate::sys::RefCountedVector::from_native_vec(lk_vec);
        for source in source_list.vec {
            sources.push(CaptureSource { ffi: source.clone() });
        }
        sources
    }
}

#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "glib-main-loop"))]
impl Drop for DesktopCapturer {
    fn drop(&mut self) {
        if let Some(glib_loop) = &self.glib_loop {
            glib_loop.quit();
        }
    }
}

pub struct DesktopCapturerCallbackWrapper {
    callback: Box<dyn DesktopCapturerCallback + Send>,
}

impl DesktopCapturerCallbackWrapper {
    pub fn new(callback: Box<dyn DesktopCapturerCallback + Send>) -> Self {
        Self { callback }
    }

    pub extern "C" fn on_capture_result(
        frame: *mut sys::lkDesktopFrame,
        result: sys::lkCaptureResult,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let callback_wrapper = unsafe { &mut *(userdata as *mut DesktopCapturerCallbackWrapper) };
        match result {
            sys::lkCaptureResult::CAPTURE_RESULT_SUCCESS => {
                let dc_frame = DesktopFrame { ffi: unsafe { sys::RefCounted::from_raw(frame) } };
                callback_wrapper.callback.on_capture_result(Ok(dc_frame))
            }
            sys::lkCaptureResult::CAPTURE_RESULT_ERROR_TEMPORARY => {
                callback_wrapper.callback.on_capture_result(Err(CaptureError::Temporary))
            }
            sys::lkCaptureResult::CAPTURE_RESULT_ERROR_PERMANENT => {
                callback_wrapper.callback.on_capture_result(Err(CaptureError::Permanent))
            }
            _ => callback_wrapper.callback.on_capture_result(Err(CaptureError::Permanent)),
        }
    }
}

impl_thread_safety!(DesktopCapturer, Send + Sync);

pub trait DesktopCapturerCallback: Send {
    fn on_capture_result(&mut self, result: Result<DesktopFrame, CaptureError>);
}

struct DesktopCallback<T: FnMut(Result<DesktopFrame, CaptureError>) + Send> {
    callback: T,
}

impl<T> DesktopCallback<T>
where
    T: FnMut(Result<DesktopFrame, CaptureError>) + Send,
{
    fn new(callback: T) -> Self {
        Self { callback }
    }

    fn capture_result_from_sys(
        result: Result<DesktopFrame, CaptureError>,
    ) -> Result<DesktopFrame, CaptureError> {
        match result {
            Ok(frame) => Ok(frame),
            Err(err) => Err(err),
        }
    }
}

impl<T> DesktopCapturerCallback for DesktopCallback<T>
where
    T: FnMut(Result<DesktopFrame, CaptureError>) + Send,
{
    fn on_capture_result(&mut self, result: Result<DesktopFrame, CaptureError>) {
        (self.callback)(DesktopCallback::<T>::capture_result_from_sys(result));
    }
}

#[derive(Clone)]
pub struct CaptureSource {
    ffi: sys::RefCounted<sys::lkDesktopSource>,
}

impl CaptureSource {
    pub fn id(&self) -> u64 {
        unsafe { sys::lkDesktopSourceGetId(self.ffi.as_ptr()) }
    }

    pub fn title(&self) -> String {
        unsafe {
            let lk_str = sys::lkDesktopSourceGetTitle(self.ffi.as_ptr());
            if lk_str.is_null() {
                return String::new();
            }
            let c_str = sys::RefCountedString::from_native(lk_str);
            c_str.as_str()
        }
    }

    pub fn display_id(&self) -> i64 {
        unsafe { sys::lkDesktopSourceGetDisplayId(self.ffi.as_ptr()) }
    }
}
