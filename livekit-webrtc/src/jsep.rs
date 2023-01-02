use std::fmt::{Debug, Formatter};

use cxx::UniquePtr;

pub use sys_jsep::ffi::{SdpParseError, SdpType};
use webrtc_sys::jsep as sys_jsep;

// TODO Maybe we can replace that by a serialized IceCandidateInit
pub struct IceCandidate {
    cxx_handle: UniquePtr<sys_jsep::ffi::IceCandidate>,
}

impl Debug for IceCandidate {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "IceCandidate[{}]", self.to_string())
    }
}

impl IceCandidate {
    pub fn from(
        sdp_mid: &str,
        sdp_mline_index: i32,
        sdp: &str,
    ) -> Result<IceCandidate, SdpParseError> {
        let res = sys_jsep::ffi::create_ice_candidate(
            sdp_mid.to_string(),
            sdp_mline_index,
            sdp.to_string(),
        );

        match res {
            Ok(cxx_handle) => Ok(IceCandidate::new(cxx_handle)),
            Err(e) => Err(unsafe { SdpParseError::from(e.what()) }),
        }
    }

    pub(crate) fn new(cxx_handle: UniquePtr<sys_jsep::ffi::IceCandidate>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn release(self) -> UniquePtr<sys_jsep::ffi::IceCandidate> {
        self.cxx_handle
    }

    pub fn sdp_mid(&self) -> String {
        self.cxx_handle.sdp_mid()
    }

    pub fn sdp_mline_index(&self) -> i32 {
        self.cxx_handle.sdp_mline_index()
    }

    pub fn candidate(&self) -> String {
        self.cxx_handle.candidate()
    }
}

impl ToString for IceCandidate {
    fn to_string(&self) -> String {
        self.cxx_handle.stringify()
    }
}

pub struct SessionDescription {
    cxx_handle: UniquePtr<sys_jsep::ffi::SessionDescription>,
}

impl Debug for SessionDescription {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "SessionDescription[{}]", self.to_string())
    }
}

impl SessionDescription {
    pub fn from(sdp_type: SdpType, description: &str) -> Result<SessionDescription, SdpParseError> {
        let res = sys_jsep::ffi::create_session_description(sdp_type, description.to_string());

        match res {
            Ok(cxx_handle) => Ok(SessionDescription::new(cxx_handle)),
            Err(e) => Err(unsafe { SdpParseError::from(e.what()) }),
        }
    }

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
