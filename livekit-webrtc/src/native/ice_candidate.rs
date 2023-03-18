use crate::ice_candidate as ic;
use crate::session_description::SdpParseError;
use cxx::SharedPtr;
use webrtc_sys::jsep as sys_jsep;

#[derive(Clone)]
pub struct IceCandidate {
    pub(crate) sys_handle: SharedPtr<sys_jsep::ffi::IceCandidate>,
}

impl IceCandidate {
    pub fn parse(
        sdp_mid: &str,
        sdp_mline_index: i32,
        sdp: &str,
    ) -> Result<ic::IceCandidate, SdpParseError> {
        let res = sys_jsep::ffi::create_ice_candidate(
            sdp_mid.to_string(),
            sdp_mline_index,
            sdp.to_string(),
        );

        match res {
            Ok(sys_handle) => Ok(ic::IceCandidate {
                handle: IceCandidate { sys_handle },
            }),
            Err(e) => Err(unsafe { sys_jsep::ffi::SdpParseError::from(e.what()).into() }),
        }
    }

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
