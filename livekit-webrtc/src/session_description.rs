use crate::imp::session_description as sd_imp;
use std::{fmt::Debug, str::FromStr};
use thiserror::Error;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SdpType {
    Offer,
    PrAnswer,
    Answer,
    Rollback,
}

impl FromStr for SdpType {
    type Err = &'static str;

    fn from_str(sdp_type: &str) -> Result<Self, Self::Err> {
        match sdp_type {
            "offer" => Ok(Self::Offer),
            "pranswer" => Ok(Self::PrAnswer),
            "answer" => Ok(Self::Answer),
            "rollback" => Ok(Self::Rollback),
            _ => Err("invalid SdpType"),
        }
    }
}

#[derive(Clone)]
pub struct SessionDescription {
    pub(crate) handle: sd_imp::SessionDescription,
}

#[derive(Clone, Error, Debug)]
#[error("Failed to parse sdp: {line} - {description}")]
pub struct SdpParseError {
    pub line: String,
    pub description: String,
}

impl SessionDescription {
    pub fn parse(sdp: &str, sdp_type: SdpType) -> Result<Self, SdpParseError> {
        sd_imp::SessionDescription::parse(sdp, sdp_type)
    }

    pub fn sdp_type(&self) -> SdpType {
        self.handle.sdp_type()
    }
}

impl ToString for SessionDescription {
    fn to_string(&self) -> String {
        self.handle.to_string()
    }
}

impl Debug for SessionDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionDescription")
            .field("sdp_type", &self.sdp_type())
            .finish()
    }
}
