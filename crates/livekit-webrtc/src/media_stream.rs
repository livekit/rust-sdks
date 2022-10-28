use cxx::UniquePtr;

use libwebrtc_sys::media_stream as sys_ms;

pub use sys_ms::ffi::TrackState;

pub struct MediaStreamTrack {
    cxx_handle: UniquePtr<sys_ms::ffi::MediaStreamTrack>,
}

impl MediaStreamTrack {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::MediaStreamTrack>) -> Self {
        Self { cxx_handle }
    }

    fn kind(&self) -> String {
        self.cxx_handle.kind()
    }

    fn id(&self) -> String {
        self.cxx_handle.id()
    }

    fn enabled(&self) -> bool {
        self.cxx_handle.enabled()
    }

    fn set_enabled(&mut self, enable: bool) -> bool {
        self.cxx_handle.pin_mut().set_enabled(enable)
    }

    fn state(&self) -> TrackState {
        self.cxx_handle.state()
    }
}

pub struct MediaStream {
    cxx_handle: UniquePtr<sys_ms::ffi::MediaStream>,
}

impl MediaStream {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::MediaStream>) -> Self {
        Self {
            cxx_handle
        }
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }
}