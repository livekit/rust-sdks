use crate::candidate::ffi::Candidate;
use crate::data_channel::ffi::DataChannel;
use crate::jsep::ffi::IceCandidate;
use crate::jsep::CreateSdpObserver;
use crate::media_stream_interface::ffi::MediaStreamInterface;
use crate::peer_connection::ffi::{
    CandidatePairChangeEvent, IceConnectionState, IceGatheringState, PeerConnectionState,
    SignalingState,
};
use crate::peer_connection::PeerConnectionObserver;
use crate::rtp_receiver::ffi::RtpReceiver;
use crate::rtp_transceiver::ffi::RtpTransceiver;
use crate::{jsep, peer_connection};
use cxx::UniquePtr;
use log::info;
use std::any::Any;
use std::thread::sleep;
use std::time::Duration;

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

        fn create_peer_connection(
            self: &PeerConnectionFactory,
            config: UniquePtr<NativeRTCConfiguration>,
            observer: UniquePtr<NativePeerConnectionObserver>,
        ) -> Result<UniquePtr<PeerConnection>>;
    }
}
