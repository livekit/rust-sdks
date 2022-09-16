use cxx::UniquePtr;
use std::fmt::{Debug, Formatter};

use crate::rtc_error::ffi::RTCError;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

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

        fn _unique_ice_candidate() -> UniquePtr<IceCandidate>; // Ignore
        fn _unique_session_description() -> UniquePtr<SessionDescription>; // Ignore
    }
}

impl Debug for ffi::SessionDescription {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.stringify())
    }
}

unsafe impl Send for ffi::SessionDescription {}

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
