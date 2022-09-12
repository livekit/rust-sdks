use cxx::UniquePtr;
use cxx::{type_id, ExternType};

use crate::rtc_error::ffi::RTCError;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    extern "Rust" {
        type CreateSdpObserverWrapper;
        fn on_success(self: &CreateSdpObserverWrapper, session_description: UniquePtr<SessionDescription>);
        fn on_failure(self: &CreateSdpObserverWrapper, error: UniquePtr<RTCError>);

        type SetLocalSdpObserverWrapper;
        fn on_set_local_description_complete(self: &SetLocalSdpObserverWrapper, error: UniquePtr<RTCError>);

        type SetRemoteSdpObserverWrapper;
        fn on_set_remote_description_complete(self: &SetRemoteSdpObserverWrapper, error: UniquePtr<RTCError>);
    }

    unsafe extern "C++" {
        include!("livekit/jsep.h");
        include!("livekit/rtc_error.h");

        type RTCError = crate::rtc_error::ffi::RTCError;
        type IceCandidate;
        type SessionDescription;
        type NativeCreateSdpObserverHandle;
        type NativeSetLocalSdpObserverHandle;
        type NativeSetRemoteSdpObserverHandle;

        fn create_native_create_sdp_observer(observer: Box<CreateSdpObserverWrapper>) -> UniquePtr<NativeCreateSdpObserverHandle>;
        fn create_native_set_local_sdp_observer(observer: Box<SetLocalSdpObserverWrapper>) -> UniquePtr<NativeSetLocalSdpObserverHandle>;
        fn create_native_set_remote_sdp_observer(observer: Box<SetRemoteSdpObserverWrapper>) -> UniquePtr<NativeSetRemoteSdpObserverHandle>;

        fn _unique_ice_candidate() -> UniquePtr<IceCandidate>; // Ignore
        fn _unique_session_description() -> UniquePtr<SessionDescription>; // Ignore
    }
}

// CreateSdpObserver

pub trait CreateSdpObserver: Send + Sync {
    fn on_success(&self, session_description: UniquePtr<ffi::SessionDescription>);
    fn on_failure(&self, error: UniquePtr<RTCError>);
}

pub struct CreateSdpObserverWrapper {
    observer: Box<dyn CreateSdpObserver>,
}

impl CreateSdpObserverWrapper {
    pub fn new(observer: Box<dyn CreateSdpObserver>) -> Self {
        Self {
            observer
        }
    }

    fn on_success(&self, session_description: UniquePtr<ffi::SessionDescription>) {
        self.observer.on_success(session_description);
    }

    fn on_failure(&self, error: UniquePtr<RTCError>) {
        self.observer.on_failure(error);
    }
}

// SetLocalSdpObserver

pub trait SetLocalSdpObserver: Send + Sync {
    fn on_set_local_description_complete(&self, error: UniquePtr<RTCError>);
}

pub struct SetLocalSdpObserverWrapper {
    observer: Box<dyn SetLocalSdpObserver>,
}

impl SetLocalSdpObserverWrapper {
    pub fn new(observer: Box<dyn SetLocalSdpObserver>) -> Self {
        Self {
            observer
        }
    }

    fn on_set_local_description_complete(&self, error: UniquePtr<RTCError>) {
        self.observer.on_set_local_description_complete(error);
    }
}

// SetRemoteSdpObserver

pub trait SetRemoteSdpObserver: Send + Sync {
    fn on_set_remote_description_complete(&self, error: UniquePtr<RTCError>);
}

pub struct SetRemoteSdpObserverWrapper {
    observer: Box<dyn SetRemoteSdpObserver>,
}

impl SetRemoteSdpObserverWrapper {
    pub fn new(observer: Box<dyn SetRemoteSdpObserver>) -> Self {
        Self {
            observer
        }
    }

    fn on_set_remote_description_complete(&self, error: UniquePtr<RTCError>) {
        self.observer.on_set_remote_description_complete(error);
    }
}
