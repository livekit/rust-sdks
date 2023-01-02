use cxx::UniquePtr;

use libwebrtc_sys::peer_connection as sys_pc;
use libwebrtc_sys::peer_connection_factory as sys_factory;
pub use sys_factory::ffi::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};

use crate::peer_connection::{InternalObserver, PeerConnection};
use crate::rtc_error::RTCError;
use crate::webrtc::RTCRuntime;

pub struct PeerConnectionFactory {
    cxx_handle: UniquePtr<sys_factory::ffi::PeerConnectionFactory>,
    rtc_runtime: RTCRuntime,
}

impl PeerConnectionFactory {
    pub fn new(rtc_runtime: RTCRuntime) -> Self {
        Self {
            cxx_handle: sys_factory::ffi::create_peer_connection_factory(
                rtc_runtime.clone().release(),
            ),
            rtc_runtime,
        }
    }

    pub fn create_peer_connection(
        &self,
        config: RTCConfiguration,
    ) -> Result<PeerConnection, RTCError> {
        let native_config = sys_factory::ffi::create_rtc_configuration(config);

        unsafe {
            let mut observer = Box::new(InternalObserver::default());
            let mut native_observer = sys_pc::ffi::create_native_peer_connection_observer(
                self.rtc_runtime.clone().release(),
                Box::new(sys_pc::PeerConnectionObserverWrapper::new(&mut *observer)),
            );

            let res = self
                .cxx_handle
                .create_peer_connection(native_config, native_observer.pin_mut());

            match res {
                Ok(cxx_handle) => Ok(PeerConnection::new(cxx_handle, observer, native_observer)),
                Err(e) => Err(RTCError::from(e.what())),
            }
        }
    }
}
