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

use crate::sys;

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum SourceType {
    Screen,
    Window,
    Generic,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct DesktopCapturerOptions {
    source_type: SourceType,
    include_cursor: bool,
    #[cfg(target_os = "macos")]
    allow_sck_system_picker: bool,
}

impl Default for DesktopCapturerOptions {
    fn default() -> Self {
        Self {
            source_type: SourceType::Screen,
            include_cursor: false,
            #[cfg(target_os = "macos")]
            allow_sck_system_picker: true,
        }
    }
}

impl DesktopCapturerOptions {
    pub(crate) fn new(source_type: SourceType) -> Self {
        Self { source_type, ..Default::default() }
    }

    pub(crate) fn with_cursor(mut self, include: bool) -> Self {
        self.include_cursor = include;
        self
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn with_sck_system_picker(mut self, allow_sck_system_picker: bool) -> Self {
        self.allow_sck_system_picker = allow_sck_system_picker;
        self
    }

    pub(crate) fn to_sys_options(&self) -> sys::lkDesktopCapturerOptions {
        let source_type = match self.source_type {
            SourceType::Screen => sys::lkSourceType::Screen,
            SourceType::Window => sys::lkSourceType::Window,
            SourceType::Generic => sys::lkSourceType::Generic,
        };
        let mut sys_options = sys::lkDesktopCapturerOptions {
            source_type,
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

pub(crate) struct DesktopCapturer {
    ffi: sys::RefCounted<sys::lkDesktopCapturer>,
    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "glib-main-loop"))]
    glib_loop: Option<glib::MainLoop>,
}

impl DesktopCapturer {
    pub(crate) fn new(options: DesktopCapturerOptions) -> Option<Self> {
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

    pub(crate) fn capture_frame(&self) {
        unsafe {
            sys::lkDesktopCapturerCaptureFrame(self.ffi.as_ptr());
        }
    }

    pub(crate) fn start<T>(&mut self, callback: T)
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
        let pin_handle = self.sys_handle.pin_mut();
        let callback = DesktopCallback::new(callback);
        let callback_wrapper = DesktopCapturerCallbackWrapper::new(Box::new(callback));
        pin_handle.start(Box::new(callback_wrapper));
    }

    pub(crate) fn select_source(&self, id: u64) -> bool {
        unsafe { sys::lkDesktopCapturerSelectSource(self.ffi.as_ptr(), id) }
    }

    pub(crate) fn get_source_list(&self) -> Vec<CaptureSource> {
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
    callback: Box<dyn sys_dc::DesktopCapturerCallback + Send>,
}

impl DesktopCapturerCallbackWrapper {
    fn new(callback: Box<dyn sys_dc::DesktopCapturerCallback + Send>) -> Self {
        Self { callback }
    }

    fn on_capture_result(
        self: &mut DesktopCapturerCallbackWrapper,
        result: CaptureResult,
        frame: UniquePtr<DesktopFrame>,
    ) {
        self.callback.on_capture_result(result, frame);
    }
}


pub(crate) struct DesktopFrame {
    ffi: sys::RefCounted<sys::lkDesktopFrame>,
}

impl DesktopFrame {
    fn new(ffi: sys::RefCounted<sys::lkDesktopFrame>) -> Self {
        Self { ffi }
    }

    pub(crate) fn width(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetWidth(self.ffi.as_ptr()) }
    }

    pub(crate) fn height(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetHeight(self.ffi.as_ptr()) }
    }

    pub(crate) fn stride(&self) -> u32 {
        unsafe { sys::lkDesktopFrameGetStride(self.ffi.as_ptr()) }
    }

    pub(crate) fn left(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetLeft(self.ffi.as_ptr()) }
    }

    pub(crate) fn top(&self) -> i32 {
        unsafe { sys::lkDesktopFrameGetTop(self.ffi.as_ptr()) }
    }

    pub(crate) fn data(&self) -> &[u8] {
        unsafe {
            let lk_data =
                sys::RefCountedData::from_native(sys::lkDesktopFrameGetData(self.ffi.as_ptr()));
            &lk_data.as_bytes()
        }
    }
}

struct DesktopCallback<
    T: FnMut(Result<sys::RefCounted<sys::lkDesktopFrame>, sys::lkCaptureError>) + Send,
> {
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
        result: Result<sys::RefCounted<sys::lkDesktopFrame>, sys::lkCaptureError>,
    ) -> Result<DesktopFrame, CaptureError> {
        match result {
            Ok(frame) => Ok(DesktopFrame::new(frame)),
            Err(error) => Err(match error {
                sys::lkCaptureError::Temporary => CaptureError::Temporary,
                sys::lkCaptureError::Permanent => CaptureError::Permanent,
            }),
        }
    }
}

impl<T> sys_dc::DesktopCapturerCallback for DesktopCallback<T>
where
    T: FnMut(Result<DesktopFrame, CaptureError>) + Send,
{
    fn on_capture_result(
        &mut self,
        result: Result<sys::RefCounted<sys::lkDesktopFrame>, sys::lkCaptureError>,
    ) {
        (self.callback)(DesktopCallback::<T>::capture_result_from_sys(result));
    }
}

#[derive(Clone)]
pub(crate) struct CaptureSource {
    ffi: sys::RefCounted<sys::lkDesktopSource>,
}

impl CaptureSource {
    pub(crate) fn id(&self) -> u64 {
        unsafe { sys::lkDesktopSourceGetId(self.ffi.as_ptr()) }
    }

    pub(crate) fn title(&self) -> String {
        unsafe {
            let lk_str = sys::lkDesktopSourceGetTitle(self.ffi.as_ptr());
            if lk_str.is_null() {
                return String::new();
            }
            let c_str = sys::RefCountedString::from_native(lk_str);
            c_str.as_str()
        }
    }

    pub(crate) fn display_id(&self) -> i64 {
        unsafe { sys::lkDesktopSourceGetDisplayId(self.ffi.as_ptr()) }
    }
}

pub(crate) enum CaptureError {
    Temporary,
    Permanent,
}
