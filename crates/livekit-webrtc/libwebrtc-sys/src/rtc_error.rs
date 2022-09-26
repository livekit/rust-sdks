use crate::rtc_error::ffi::RTCErrorType;
use std::error::Error;
use std::fmt::{Display, Formatter};

// cxx doesn't support custom Exception type, so we serialize RTCError inside the cxx::Exception "what" string

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum RTCErrorType {
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
    pub enum RTCErrorDetailType {
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
    pub struct RTCError {
        pub error_type: RTCErrorType,
        pub message: String,
        pub error_detail: RTCErrorDetailType,
        pub has_sctp_cause_code: bool,
        // cxx doesn't support the Option trait
        pub sctp_cause_code: u16,
    }
}

impl ffi::RTCError {
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
            error_type: unsafe { std::mem::transmute(error_type) },
            error_detail: unsafe { std::mem::transmute(error_detail) },
            sctp_cause_code,
            has_sctp_cause_code: has_scp_cause_code == 1,
            message,
        }
    }

    pub fn ok(&self) -> bool {
        return self.error_type == RTCErrorType::None;
    }
}

impl Error for ffi::RTCError {}

impl Display for ffi::RTCError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(
            f,
            "RtcError occurred {:?}: {}",
            self.error_type, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::rtc_error::ffi::{RTCError, RTCErrorDetailType, RTCErrorType};

    #[cxx::bridge(namespace = "livekit")]
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
        let error = unsafe { RTCError::from(&str) };

        assert_eq!(error.error_type, RTCErrorType::InternalError);
        assert_eq!(error.error_detail, RTCErrorDetailType::DataChannelFailure);
        assert_eq!(error.has_sctp_cause_code, true);
        assert_eq!(error.sctp_cause_code, 24);
        assert_eq!(
            error.message,
            "this is not a test, I repeat, this is not a test"
        );
    }

    #[test]
    fn throw_error() {
        let exc: cxx::Exception = ffi::throw_error().err().unwrap();
        let error = unsafe { RTCError::from(exc.what()) };

        assert_eq!(error.error_type, RTCErrorType::InvalidModification);
        assert_eq!(error.error_detail, RTCErrorDetailType::None);
        assert_eq!(error.has_sctp_cause_code, false);
        assert_eq!(error.sctp_cause_code, 0);
        assert_eq!(error.message, "exception is thrown!");
    }
}
