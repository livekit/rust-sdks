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

use crate::impl_thread_safety;
use crate::session_description::SdpParseError;
use crate::sys;
use std::fmt::Debug;

#[derive(Clone)]
pub struct IceCandidate {
    pub(crate) ffi: sys::RefCounted<sys::lkIceCandidate>,
}

impl_thread_safety!(IceCandidate, Send + Sync);

impl IceCandidate {
    pub fn parse(
        sdp_mid: &str,
        sdp_mline_index: i32,
        sdp: &str,
    ) -> Result<IceCandidate, SdpParseError> {
        let c_sdp = std::ffi::CString::new(sdp).map_err(|e| SdpParseError {
            line: sdp.lines().next().unwrap_or("").to_string(),
            description: format!("Failed to convert SDP to CString: {}", e),
        })?;
        let c_sdp_mid = std::ffi::CString::new(sdp_mid).map_err(|e| SdpParseError {
            line: sdp_mid.to_string(),
            description: format!("Failed to convert SDP mid to CString: {}", e),
        })?;

        let ffi = unsafe {
            sys::lkCreateIceCandidate(c_sdp_mid.as_ptr(), sdp_mline_index, c_sdp.as_ptr())
        };
        Ok(IceCandidate { ffi: unsafe { sys::RefCounted::from_raw(ffi) } })
    }

    pub fn sdp_mid(&self) -> String {
        unsafe {
            let str_ptr = sys::lkIceCandidateGetMid(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }

    pub fn sdp_mline_index(&self) -> i32 {
        unsafe { sys::lkIceCandidateGetMlineIndex(self.ffi.as_ptr()) }
    }

    pub fn candidate(&self) -> String {
        unsafe {
            let str_ptr = sys::lkIceCandidateGetSdp(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }
}

impl ToString for IceCandidate {
    fn to_string(&self) -> String {
        self.candidate()
    }
}

impl Debug for IceCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IceCandidate").field("candidate", &self.to_string()).finish()
    }
}
