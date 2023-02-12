#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    // Wrapper to opaque C++ objects
    // https://github.com/dtolnay/cxx/issues/741
    // Used to allow SharedPtr/UniquePtr type inside a rust::Vec
    pub struct MediaStreamPtr {
        pub ptr: SharedPtr<MediaStream>,
    }

    pub struct CandidatePtr {
        pub ptr: SharedPtr<Candidate>,
    }

    pub struct AudioTrackPtr {
        pub ptr: SharedPtr<AudioTrack>,
    }

    pub struct VideoTrackPtr {
        pub ptr: SharedPtr<VideoTrack>,
    }

    pub struct RtpSenderPtr {
        pub ptr: SharedPtr<RtpSender>,
    }

    pub struct RtpReceiverPtr {
        pub ptr: SharedPtr<RtpReceiver>,
    }

    pub struct RtpTransceiverPtr {
        pub ptr: SharedPtr<RtpTransceiver>,
    }

    unsafe extern "C++" {
        include!("livekit/helper.h");

        type MediaStream = crate::media_stream::ffi::MediaStream;
        type AudioTrack = crate::media_stream::ffi::AudioTrack;
        type VideoTrack = crate::media_stream::ffi::VideoTrack;
        type Candidate = crate::candidate::ffi::Candidate;
        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type RtpTransceiver = crate::rtp_transceiver::ffi::RtpTransceiver;

        fn _vec_media_stream_ptr() -> Vec<MediaStreamPtr>;
        fn _vec_candidate_ptr() -> Vec<CandidatePtr>;
        fn _vec_audio_track_ptr() -> Vec<AudioTrackPtr>;
        fn _vec_video_track_ptr() -> Vec<VideoTrackPtr>;
        fn _vec_rtp_sender_ptr() -> Vec<RtpSenderPtr>;
        fn _vec_rtp_receiver_ptr() -> Vec<RtpReceiverPtr>;
        fn _vec_rtp_transceiver_ptr() -> Vec<RtpTransceiverPtr>;
    }
}
