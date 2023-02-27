use std::fmt::Debug;

use crate::imp::ice_candidate as imp_ic;

pub struct IceCandidate {
    pub(crate) handle: imp_ic::IceCandidate,
}

impl IceCandidate {
    pub fn sdp_mid(&self) -> String {
        self.handle.sdp_mid()
    }

    pub fn sdp_mline_index(&self) -> i32 {
        self.handle.sdp_mline_index()
    }

    pub fn candidate(&self) -> String {
        self.handle.candidate()
    }
}

impl ToString for IceCandidate {
    fn to_string(&self) -> String {
        self.handle.to_string()
    }
}

impl Debug for IceCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IceCandidate").finish()
    }
}
