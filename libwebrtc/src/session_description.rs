// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use thiserror::Error;

use crate::sys::{self, lkSdpType};

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

impl From<SdpType> for sys::lkSdpType {
    fn from(sdp_type: SdpType) -> Self {
        match sdp_type {
            SdpType::Offer => sys::lkSdpType::LK_SDP_TYPE_OFFER,
            SdpType::PrAnswer => sys::lkSdpType::LK_SDP_TYPE_PRANSWER,
            SdpType::Answer => sys::lkSdpType::LK_SDP_TYPE_ANSWER,
            SdpType::Rollback => sys::lkSdpType::LK_SDP_TYPE_ROLLBACK,
        }
    }
}

impl From<lkSdpType> for SdpType {
    fn from(sdp_type: lkSdpType) -> Self {
        match sdp_type {
            lkSdpType::LK_SDP_TYPE_OFFER => SdpType::Offer,
            lkSdpType::LK_SDP_TYPE_PRANSWER => SdpType::PrAnswer,
            lkSdpType::LK_SDP_TYPE_ANSWER => SdpType::Answer,
            lkSdpType::LK_SDP_TYPE_ROLLBACK => SdpType::Rollback,
        }
    }
}

impl Display for SdpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SdpType::Offer => "offer",
            SdpType::PrAnswer => "pranswer",
            SdpType::Answer => "answer",
            SdpType::Rollback => "rollback",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone)]
pub struct SessionDescription {
    pub(crate) ffi: sys::RefCounted<sys::lkSessionDescription>,
}

#[derive(Clone, Error, Debug)]
#[error("Failed to parse sdp: {line} - {description}")]
pub struct SdpParseError {
    pub line: String,
    pub description: String,
}

impl SessionDescription {
    pub fn parse(sdp: &str, sdp_type: SdpType) -> Result<Self, SdpParseError> {
        // Basic validation: check if sdp starts with "v="
        if !sdp.starts_with("v=") {
            return Err(SdpParseError {
                line: sdp.lines().next().unwrap_or("").to_string(),
                description: "SDP must start with 'v='".to_string(),
            });
        }

        let c_sdp = std::ffi::CString::new(sdp).map_err(|e| SdpParseError {
            line: sdp.lines().next().unwrap_or("").to_string(),
            description: format!("Failed to convert SDP to CString: {}", e),
        })?;
        let desc = unsafe { sys::lkCreateSessionDescription(sdp_type.into(), c_sdp.as_ptr()) };

        Ok(SessionDescription { ffi: unsafe { sys::RefCounted::from_raw(desc) } })
    }

    pub fn sdp_type(&self) -> SdpType {
        unsafe { sys::lkSessionDescriptionGetType(self.ffi.as_ptr()).into() }
    }

    pub fn sdp(&self) -> String {
        unsafe {
            let str_ptr = sys::lkSessionDescriptionGetSdp(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }
}

impl ToString for SessionDescription {
    fn to_string(&self) -> String {
        self.sdp()
    }
}
impl Debug for SessionDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionDescription").field("sdp_type", &self.sdp_type()).finish()
    }
}
