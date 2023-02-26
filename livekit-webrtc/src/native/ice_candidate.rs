use cxx::SharedPtr;
use webrtc_sys::jsep as sys_jsep;

#[derive(Clone)]
pub struct IceCandidate {
    sys_handle: SharedPtr<sys_jsep::ffi::IceCandidate>,
}

impl IceCandidate {
    pub fn sdp_mid(&self) -> String {
        self.sys_handle.sdp_mid()
    }

    pub fn sdp_mline_index(&self) -> i32 {
        self.sys_handle.sdp_mline_index()
    }

    pub fn candidate(&self) -> String {
        self.sys_handle.candidate()
    }
}

impl ToString for IceCandidate {
    fn to_string(&self) -> String {
        self.sys_handle.stringify()
    }
}
