use crate::session_description::SdpType;
use cxx::UniquePtr;
use webrtc_sys::jsep as sys_jsep;

impl From<sys_jsep::ffi::SdpType> for SdpType {
    fn from(sdp_type: sys_jsep::ffi::SdpType) -> Self {
        match sdp_type {
            sys_jsep::ffi::SdpType::Offer => SdpType::Offer,
            sys_jsep::ffi::SdpType::PrAnswer => SdpType::PrAnswer,
            sys_jsep::ffi::SdpType::Answer => SdpType::Answer,
            sys_jsep::ffi::SdpType::Rollback => SdpType::Rollback,
            _ => panic!("unknown SdpType"),
        }
    }
}

pub struct SessionDescription {
    pub(crate) sys_handle: UniquePtr<sys_jsep::ffi::SessionDescription>,
}

impl SessionDescription {
    pub fn sdp_type(&self) -> SdpType {
        self.sys_handle.sdp_type().into()
    }
}

impl ToString for SessionDescription {
    fn to_string(&self) -> String {
        self.sys_handle.stringify()
    }
}

impl Clone for SessionDescription {
    fn clone(&self) -> Self {
        SessionDescription {
            sys_handle: self.sys_handle.clone(),
        }
    }
}
