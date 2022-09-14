use cxx::{SharedPtr, UniquePtr};
use libwebrtc_sys::jsep as sys_jsep;

#[derive(Debug)]
pub struct IceCandidate {

}

#[derive(Debug)]
pub struct SessionDescription {
    cxx_handle: UniquePtr<sys_jsep::ffi::SessionDescription>
}

impl SessionDescription {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_jsep::ffi::SessionDescription>) -> Self {
        Self {
            cxx_handle
        }
    }

    pub(crate) fn release(self) -> UniquePtr<sys_jsep::ffi::SessionDescription>{
        self.cxx_handle
    }
}