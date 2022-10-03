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

        fn create_peer_connection_factory(runtime: SharedPtr<RTCRuntime>) -> UniquePtr<PeerConnectionFactory>;
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

unsafe impl Send for ffi::PeerConnectionFactory {}

unsafe impl Sync for ffi::PeerConnectionFactory {}
