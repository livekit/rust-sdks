use cxx::UniquePtr;
use std::fmt::{Debug, Formatter};

use libwebrtc_sys::media_stream as sys_ms;
pub use sys_ms::ffi::TrackState;

pub struct MediaStreamTrack {
    cxx_handle: UniquePtr<sys_ms::ffi::MediaStreamTrack>,
}

impl Debug for MediaStreamTrack {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("MediaStreamTrack")
            .field("id", &self.id())
            .field("kind", &self.kind())
            .field("enabled", &self.enabled())
            .field("state", &self.state())
            .finish()
    }
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

impl Debug for MediaStream {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("MediaStream")
            .field("id", &self.id())
            .finish()
    }
}

impl MediaStream {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_ms::ffi::MediaStream>) -> Self {
        Self { cxx_handle }
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }
}
