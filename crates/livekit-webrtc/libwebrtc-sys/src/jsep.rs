use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

use cxx::UniquePtr;

use crate::rtc_error::ffi::RTCError;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum SdpType {
        Offer,
        PrAnswer,
        Answer,
        Rollback,
    }

    #[derive(Debug)]
    pub struct SdpParseError {
        pub line: String,
        pub description: String,
    }

    extern "Rust" {
        type CreateSdpObserverWrapper;
        fn on_success(
            self: &CreateSdpObserverWrapper,
            session_description: UniquePtr<SessionDescription>,
        );
        fn on_failure(self: &CreateSdpObserverWrapper, error: RTCError);

        type SetLocalSdpObserverWrapper;
        fn on_set_local_description_complete(self: &SetLocalSdpObserverWrapper, error: RTCError);

        type SetRemoteSdpObserverWrapper;
        fn on_set_remote_description_complete(self: &SetRemoteSdpObserverWrapper, error: RTCError);
    }

    unsafe extern "C++" {
        include!("libwebrtc-sys/src/rtc_error.rs.h");
        include!("livekit/jsep.h");

        type RTCError = crate::rtc_error::ffi::RTCError;
        type IceCandidate;
        type SessionDescription;
        type NativeCreateSdpObserverHandle;
        type NativeSetLocalSdpObserverHandle;
        type NativeSetRemoteSdpObserverHandle;

        fn sdp_mid(self: &IceCandidate) -> String;
        fn sdp_mline_index(self: &IceCandidate) -> i32;
        fn candidate(self: &IceCandidate) -> String;
        fn stringify(self: &IceCandidate) -> String;

        fn stringify(self: &SessionDescription) -> String;
        fn clone(self: &SessionDescription) -> UniquePtr<SessionDescription>;

        fn create_native_create_sdp_observer(
            observer: Box<CreateSdpObserverWrapper>,
        ) -> UniquePtr<NativeCreateSdpObserverHandle>;
        fn create_native_set_local_sdp_observer(
            observer: Box<SetLocalSdpObserverWrapper>,
        ) -> UniquePtr<NativeSetLocalSdpObserverHandle>;
        fn create_native_set_remote_sdp_observer(
            observer: Box<SetRemoteSdpObserverWrapper>,
        ) -> UniquePtr<NativeSetRemoteSdpObserverHandle>;

        fn create_ice_candidate(sdp_mid: String, sdp_mline_index: i32, sdp: String) -> Result<UniquePtr<IceCandidate>>;
        fn create_session_description(sdp_type: SdpType, sdp: String) -> Result<UniquePtr<SessionDescription>>;

        fn _unique_ice_candidate() -> UniquePtr<IceCandidate>; // Ignore
    fn _unique_session_description() -> UniquePtr<SessionDescription>; // Ignore
    }
}

impl Error for ffi::SdpParseError {}

impl Display for ffi::SdpParseError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "SdpParseError occurred {}: {}", self.line, self.description)
    }
}

unsafe impl Send for ffi::SessionDescription {}

unsafe impl Sync for ffi::SessionDescription {}

unsafe impl Send for ffi::IceCandidate {}

unsafe impl Sync for ffi::IceCandidate {}

impl ffi::SdpParseError {
    /// # Safety
    /// The value must be correctly encoded
    pub unsafe fn from(value: &str) -> Self {
        // Parse the hex encoded error from c++
        let line_length = u32::from_str_radix(&value[0..8], 16).unwrap() as usize + 8;
        let line = String::from(&value[8..line_length]);
        let description = String::from(&value[line_length..]);

        Self {
            line,
            description,
        }
    }
}

impl FromStr for ffi::SdpType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "offer" => Ok(ffi::SdpType::Offer),
            "pranswer" => Ok(ffi::SdpType::PrAnswer),
            "answer" => Ok(ffi::SdpType::Answer),
            "rollback" => Ok(ffi::SdpType::Rollback),
            _ => Err(()),
        }
    }
}

// CreateSdpObserver

pub trait CreateSdpObserver: Send {
    fn on_success(&self, session_description: UniquePtr<ffi::SessionDescription>);
    fn on_failure(&self, error: RTCError);
}

pub struct CreateSdpObserverWrapper {
    observer: Box<dyn CreateSdpObserver>,
}

impl CreateSdpObserverWrapper {
    pub fn new(observer: Box<dyn CreateSdpObserver>) -> Self {
        Self { observer }
    }

    fn on_success(&self, session_description: UniquePtr<ffi::SessionDescription>) {
        self.observer.on_success(session_description);
    }

    fn on_failure(&self, error: RTCError) {
        self.observer.on_failure(error);
    }
}

// SetLocalSdpObserver

pub trait SetLocalSdpObserver: Send {
    fn on_set_local_description_complete(&self, error: RTCError);
}

pub struct SetLocalSdpObserverWrapper {
    observer: Box<dyn SetLocalSdpObserver>,
}

impl SetLocalSdpObserverWrapper {
    pub fn new(observer: Box<dyn SetLocalSdpObserver>) -> Self {
        Self { observer }
    }

    fn on_set_local_description_complete(&self, error: RTCError) {
        self.observer.on_set_local_description_complete(error);
    }
}

// SetRemoteSdpObserver

pub trait SetRemoteSdpObserver: Send {
    fn on_set_remote_description_complete(&self, error: RTCError);
}

pub struct SetRemoteSdpObserverWrapper {
    observer: Box<dyn SetRemoteSdpObserver>,
}

impl SetRemoteSdpObserverWrapper {
    pub fn new(observer: Box<dyn SetRemoteSdpObserver>) -> Self {
        Self { observer }
    }

    fn on_set_remote_description_complete(&self, error: RTCError) {
        self.observer.on_set_remote_description_complete(error);
    }
}

#[cfg(test)]
mod tests {
    use log::info;

    use crate::jsep::ffi;

    #[test]
    fn throw_error() {
        let sdp_string = "v=0
o=- 6549709950142776241 2 IN IP4 127.0.0.1
s=-
t=0 0
======================== ERROR HERE
a=group:BUNDLE 0
a=extmap-allow-mixed
a=msid-semantic: WMS
m=application 9 UDP/DTLS/SCTP webrtc-datachannel
c=IN IP4 0.0.0.0
a=ice-ufrag:Tw7h
a=ice-pwd:6XOVUD6HpcB4c1M8EB8jXJE9
a=ice-options:trickle
a=fingerprint:sha-256 4F:EC:23:59:5D:A5:E6:3E:3E:5D:8A:09:B6:FA:04:AA:19:99:49:67:BD:65:93:06:BB:EE:AC:D5:21:0F:57:D6
a=setup:actpass
a=mid:0
a=sctp-port:5000
a=max-message-size:262144
";

        let sdp = ffi::create_session_description(ffi::SdpType::Offer, sdp_string.to_string());
        let err = unsafe { ffi::SdpParseError::from(sdp.err().unwrap().what()) };
        info!("parse err: {:?}", err)
    }
}
