use std::any::Any;

use crate::jsep::CreateSdpObserver;
use crate::peer_connection::PeerConnectionObserver;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug, Clone)]
    pub struct ICEServer {
        pub urls: Vec<String>,
        pub username: String,
        pub password: String,
    }

    #[derive(Debug, Clone)]
    pub struct RTCConfiguration {
        pub ice_servers: Vec<ICEServer>,
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection_factory.h");

        type PeerConnection = crate::peer_connection::ffi::PeerConnection;
        type NativePeerConnectionObserver =
        crate::peer_connection::ffi::NativePeerConnectionObserver;
        type PeerConnectionFactory;
        type NativeRTCConfiguration;

        fn create_peer_connection_factory() -> UniquePtr<PeerConnectionFactory>;
        fn create_rtc_configuration(conf: RTCConfiguration) -> UniquePtr<NativeRTCConfiguration>;

        /// SAFETY
        /// The observer must live as long as the PeerConnection
        unsafe fn create_peer_connection(
            self: &PeerConnectionFactory,
            config: UniquePtr<NativeRTCConfiguration>,
            observer: Pin<&mut NativePeerConnectionObserver>,
        ) -> Result<UniquePtr<PeerConnection>>;
    }
}
