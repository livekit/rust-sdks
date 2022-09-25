use cxx::UniquePtr;

use libwebrtc_sys::jsep as sys_jsep;

// TODO Maybe we can replace that by a serialized IceCandidateInit
#[derive(Debug)]
pub struct IceCandidate {
    cxx_handle: UniquePtr<sys_jsep::ffi::IceCandidate>,
}

impl IceCandidate {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_jsep::ffi::IceCandidate>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<sys_jsep::ffi::IceCandidate> {
        self.cxx_handle
    }
}

impl ToString for IceCandidate {
    fn to_string(&self) -> String {
        self.cxx_handle.stringify()
    }
}

#[derive(Debug)]
pub struct SessionDescription {
    cxx_handle: UniquePtr<sys_jsep::ffi::SessionDescription>,
}

impl SessionDescription {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_jsep::ffi::SessionDescription>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<sys_jsep::ffi::SessionDescription> {
        self.cxx_handle
    }
}

impl ToString for SessionDescription {
    fn to_string(&self) -> String {
        self.cxx_handle.stringify()
    }
}

impl Clone for SessionDescription {
    fn clone(&self) -> Self {
        SessionDescription::new(self.cxx_handle.clone())
    }
}
