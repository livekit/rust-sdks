use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
        include!("livekit/media_stream.h");
        include!("livekit/webrtc.h");
        include!("livekit/peer_connection_factory.h");
        include!("livekit/rtp_parameters.h");

        type AudioTrackSource = crate::audio_track::ffi::AudioTrackSource;
        type VideoTrackSource = crate::video_track::ffi::VideoTrackSource;
        type AudioTrack = crate::audio_track::ffi::AudioTrack;
        type VideoTrack = crate::video_track::ffi::VideoTrack;
        type RtpCapabilities = crate::rtp_parameters::ffi::RtpCapabilities;
        type MediaType = crate::webrtc::ffi::MediaType;
        type NativePeerConnectionObserver =
            crate::peer_connection::ffi::NativePeerConnectionObserver;
        type RtcConfiguration = crate::peer_connection::ffi::RtcConfiguration;
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection_factory.h");

        type PeerConnection = crate::peer_connection::ffi::PeerConnection;
        type PeerConnectionFactory;

        fn create_peer_connection_factory() -> SharedPtr<PeerConnectionFactory>;

        fn create_peer_connection(
            self: &PeerConnectionFactory,
            config: RtcConfiguration,
            observer: UniquePtr<NativePeerConnectionObserver>,
        ) -> Result<SharedPtr<PeerConnection>>;

        fn create_video_track(
            self: &PeerConnectionFactory,
            label: String,
            source: SharedPtr<VideoTrackSource>,
        ) -> SharedPtr<VideoTrack>;

        fn create_audio_track(
            self: &PeerConnectionFactory,
            label: String,
            source: SharedPtr<AudioTrackSource>,
        ) -> SharedPtr<AudioTrack>;

        fn rtp_sender_capabilities(
            self: &PeerConnectionFactory,
            kind: MediaType,
        ) -> RtpCapabilities;

        fn rtp_receiver_capabilities(
            self: &PeerConnectionFactory,
            kind: MediaType,
        ) -> RtpCapabilities;
    }
}

impl_thread_safety!(ffi::PeerConnectionFactory, Send + Sync);
