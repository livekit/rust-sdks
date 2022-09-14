use cxx::UniquePtr;
use libwebrtc_sys::peer_connection as sys_pc;
use libwebrtc_sys::peer_connection_factory as sys_factory;

use crate::peer_connection::PeerConnection;
use crate::rtc_error::RTCError;

pub use sys_factory::ffi::{ICEServer, RTCConfiguration};

pub struct PeerConnectionFactory {
    cxx_handle: UniquePtr<sys_factory::ffi::PeerConnectionFactory>,
}

impl PeerConnectionFactory {
    pub fn new() -> Self {
        Self {
            cxx_handle: sys_factory::ffi::create_peer_connection_factory(),
        }
    }

    pub fn create_peer_connection(
        &self,
        config: RTCConfiguration,
        observer: Box<dyn sys_pc::PeerConnectionObserver>,
    ) -> Result<PeerConnection, RTCError> {
        let native_config = sys_factory::ffi::create_rtc_configuration(config);
        let native_observer = sys_pc::ffi::create_native_peer_connection_observer(Box::new(
            sys_pc::PeerConnectionObserverWrapper::new(observer),
        ));

        let pc_result: Result<UniquePtr<sys_pc::ffi::PeerConnection>, cxx::Exception> = unsafe {
            self.cxx_handle
                .create_peer_connection(native_config, native_observer)
        };

        match pc_result {
            Ok(cxx_handle) => Ok(PeerConnection::new(cxx_handle)),
            Err(e) => {
                Err(unsafe { RTCError::from(e.what()) }) // TODO
            }
        }
    }
}
