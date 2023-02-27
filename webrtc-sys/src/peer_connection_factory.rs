use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug, Clone)]
    pub struct ICEServer {
        pub urls: Vec<String>,
        pub username: String,
        pub password: String,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum ContinualGatheringPolicy {
        GatherOnce,
        GatherContinually,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum IceTransportsType {
        None,
        Relay,
        NoHost,
        All,
    }

    #[derive(Debug, Clone)]
    pub struct RTCConfiguration {
        pub ice_servers: Vec<ICEServer>,
        pub continual_gathering_policy: ContinualGatheringPolicy,
        pub ice_transport_type: IceTransportsType,
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection_factory.h");

        type PeerConnection = crate::peer_connection::ffi::PeerConnection;
        type NativePeerConnectionObserver =
            crate::peer_connection::ffi::NativePeerConnectionObserver;
        type PeerConnectionFactory;
        type NativeRTCConfiguration;
        type RTCRuntime = crate::webrtc::ffi::RTCRuntime;

        fn create_peer_connection_factory(
            runtime: SharedPtr<RTCRuntime>,
        ) -> SharedPtr<PeerConnectionFactory>;
        fn create_rtc_configuration(conf: RTCConfiguration) -> UniquePtr<NativeRTCConfiguration>;

        /// # Safety
        /// The observer must live as long as the PeerConnection
        unsafe fn create_peer_connection(
            self: &PeerConnectionFactory,
            config: UniquePtr<NativeRTCConfiguration>,
            observer: *mut NativePeerConnectionObserver,
        ) -> Result<SharedPtr<PeerConnection>>;
    }
}

impl_thread_safety!(ffi::PeerConnectionFactory, Send + Sync);
