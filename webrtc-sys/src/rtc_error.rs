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
    error::Error,
    fmt::{Display, Formatter},
};

// cxx doesn't support custom Exception type, so we serialize RtcError inside the cxx::Exception
// "what" string

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtcErrorType {
        None,
        UnsupportedOperation,
        UnsupportedParameter,
        InvalidParameter,
        InvalidRange,
        SyntaxError,
        InvalidState,
        InvalidModification,
        NetworkError,
        ResourceExhausted,
        InternalError,
        OperationErrorWithData,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum RtcErrorDetailType {
        None,
        DataChannelFailure,
        DtlsFailure,
        FingerprintFailure,
        SctpFailure,
        SdpSyntaxError,
        HardwareEncoderNotAvailable,
        HardwareEncoderError,
    }

    #[derive(Debug)]
    pub struct RtcError {
        pub error_type: RtcErrorType,
        pub message: String,
        pub error_detail: RtcErrorDetailType,
        // cxx doesn't support the Option trait
        pub has_sctp_cause_code: bool,
        pub sctp_cause_code: u16,
    }
}

impl ffi::RtcError {
    /// # Safety
    /// The value must be correctly encoded
    pub unsafe fn from(value: &str) -> Self {
        // Parse the hex encoded error from c++
        let error_type = u32::from_str_radix(&value[0..8], 16).unwrap();
        let error_detail = u32::from_str_radix(&value[8..16], 16).unwrap();
        let has_scp_cause_code = u8::from_str_radix(&value[16..18], 16).unwrap();
        let sctp_cause_code = u16::from_str_radix(&value[18..22], 16).unwrap();
        let message = String::from(&value[22..]); // msg isn't encoded

        Self {
            error_type: std::mem::transmute(error_type),
            error_detail: std::mem::transmute(error_detail),
            sctp_cause_code,
            has_sctp_cause_code: has_scp_cause_code == 1,
            message,
        }
    }

    pub fn ok(&self) -> bool {
        self.error_type == ffi::RtcErrorType::None
    }
}

impl Error for ffi::RtcError {}

impl Display for ffi::RtcError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "RtcError occurred {:?}: {}", self.error_type, self.message)
    }
}

#[cfg(test)]
mod tests {
    use crate::rtc_error::ffi::{RtcError, RtcErrorDetailType, RtcErrorType};

    #[cxx::bridge(namespace = "livekit_ffi")]
    pub mod ffi {
        unsafe extern "C++" {
            include!("livekit/rtc_error.h");

            fn serialize_deserialize() -> String;
            fn throw_error() -> Result<()>;
        }
    }

    #[test]
    fn serialize_deserialize() {
        let str = ffi::serialize_deserialize();
        let error = unsafe { RtcError::from(&str) };

        assert_eq!(error.error_type, RtcErrorType::InternalError);
        assert_eq!(error.error_detail, RtcErrorDetailType::DataChannelFailure);
        assert!(error.has_sctp_cause_code);
        assert_eq!(error.sctp_cause_code, 24);
        assert_eq!(error.message, "this is not a test, I repeat, this is not a test");
    }

    #[test]
    fn throw_error() {
        let exc: cxx::Exception = ffi::throw_error().err().unwrap();
        let error = unsafe { RtcError::from(exc.what()) };

        assert_eq!(error.error_type, RtcErrorType::InvalidModification);
        assert_eq!(error.error_detail, RtcErrorDetailType::None);
        assert!(!error.has_sctp_cause_code);
        assert_eq!(error.sctp_cause_code, 0);
        assert_eq!(error.message, "exception is thrown!");
    }
}
