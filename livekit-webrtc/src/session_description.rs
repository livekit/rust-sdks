use crate::imp::session_description as sd_imp;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SdpType {
    Offer,
    PrAnswer,
    Answer,
    Rollback,
}

#[derive(Clone)]
pub struct SessionDescription {
    handle: sd_imp::SessionDescription,
}

impl SessionDescription {
    pub fn sdp_type(&self) -> SdpType {
        self.handle.sdp_type()
    }
}

impl ToString for SessionDescription {
    fn to_string(&self) -> String {
        self.handle.to_string()
    }
}
