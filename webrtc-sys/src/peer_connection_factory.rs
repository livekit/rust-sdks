use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug, Clone)]
    pub struct IceServer {
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
    pub struct RtcConfiguration {
        pub ice_servers: Vec<IceServer>,
        pub continual_gathering_policy: ContinualGatheringPolicy,
        pub ice_transport_type: IceTransportsType,
    }

    extern "C++" {
        include!("livekit/media_stream.h");
        include!("livekit/webrtc.h");
        include!("livekit/rtp_parameters.h");

        type AudioTrackSource = crate::media_stream::ffi::AudioTrackSource;
        type AdaptedVideoTrackSource = crate::media_stream::ffi::AdaptedVideoTrackSource;
        type AudioTrack = crate::media_stream::ffi::AudioTrack;
        type VideoTrack = crate::media_stream::ffi::VideoTrack;
        type RtpCapabilities = crate::rtp_parameters::ffi::RtpCapabilities;
        type MediaType = crate::webrtc::ffi::MediaType;
        type BoxPeerConnectionObserver = crate::peer_connection::ffi::BoxPeerConnectionObserver;
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection_factory.h");

        type PeerConnection = crate::peer_connection::ffi::PeerConnection;
        type PeerConnectionFactory;

        fn create_peer_connection_factory() -> SharedPtr<PeerConnectionFactory>;

        /// # Safety
        /// The observer must live as long as the PeerConnection does
        unsafe fn create_peer_connection(
            self: &PeerConnectionFactory,
            config: RtcConfiguration,
            observer: Box<BoxPeerConnectionObserver>,
        ) -> Result<SharedPtr<PeerConnection>>;

        fn create_video_track(
            self: &PeerConnectionFactory,
            label: String,
            source: SharedPtr<AdaptedVideoTrackSource>,
        ) -> SharedPtr<VideoTrack>;

        fn create_audio_track(
            self: &PeerConnectionFactory,
            label: String,
            source: SharedPtr<AudioTrackSource>,
        ) -> SharedPtr<AudioTrack>;

        fn get_rtp_sender_capabilities(
            self: &PeerConnectionFactory,
            kind: MediaType,
        ) -> RtpCapabilities;

        fn get_rtp_receiver_capabilities(
            self: &PeerConnectionFactory,
            kind: MediaType,
        ) -> RtpCapabilities;
    }
}

impl_thread_safety!(ffi::PeerConnectionFactory, Send + Sync);
