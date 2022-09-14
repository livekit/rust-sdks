use std::cell::RefCell;
use std::rc::Rc;
use crate::candidate::ffi::Candidate;
use crate::data_channel::ffi::DataChannel;
use crate::jsep::ffi::IceCandidate;
use crate::media_stream_interface::ffi::MediaStreamInterface;
use crate::rtp_receiver::ffi::RtpReceiver;
use crate::rtp_transceiver::ffi::RtpTransceiver;
use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    struct CandidatePair {
        local: UniquePtr<Candidate>,
        remote: UniquePtr<Candidate>,
    }

    struct CandidatePairChangeEvent {
        selected_candidate_pair: CandidatePair,
        last_data_received_ms: i64,
        reason: String,
        estimated_disconnected_time_ms: i64,
    }

    #[derive(Debug)]
    #[repr(u32)]
    pub enum PeerConnectionState {
        New,
        Connecting,
        Connected,
        Disconnected,
        Failed,
        Closed,
    }

    #[derive(Debug)]
    #[repr(u32)]
    pub enum SignalingState {
        Stable,
        HaveLocalOffer,
        HaveLocalPrAnswer,
        HaveRemoteOffer,
        HaveRemotePrAnswer,
        Closed,
    }

    #[derive(Debug)]
    #[repr(u32)]
    pub enum IceConnectionState {
        IceConnectionNew,
        IceConnectionChecking,
        IceConnectionConnected,
        IceConnectionCompleted,
        IceConnectionFailed,
        IceConnectionDisconnected,
        IceConnectionClosed,
        IceConnectionMax,
    }

    #[derive(Debug)]
    #[repr(u32)]
    pub enum IceGatheringState {
        IceGatheringNew,
        IceGatheringGathering,
        IceGatheringComplete,
    }

    #[derive(Debug)]
    pub struct RTCOfferAnswerOptions {
        offer_to_receive_video: i32,
        offer_to_receive_audio: i32,
        voice_activity_detection: bool,
        ice_restart: bool,
        use_rtp_mux: bool,
        raw_packetization_for_video: bool,
        num_simulcast_layers: i32,
        use_obsolete_sctp_sdp: bool,
    }

    // Wrapper to opaque C++ objects
    // https://github.com/dtolnay/cxx/issues/741
    struct MediaStreamPtr {
        pub ptr: UniquePtr<MediaStreamInterface>,
    }

    struct CandidatePtr {
        pub ptr: UniquePtr<Candidate>,
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection.h");
        include!("livekit/jsep.h");
        include!("livekit/data_channel.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/rtp_transceiver.h");
        include!("livekit/media_stream_interface.h");
        include!("livekit/candidate.h");

        type Candidate = crate::candidate::ffi::Candidate;
        type IceCandidate = crate::jsep::ffi::IceCandidate;
        type DataChannel = crate::data_channel::ffi::DataChannel;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type RtpTransceiver = crate::rtp_transceiver::ffi::RtpTransceiver;
        type MediaStreamInterface = crate::media_stream_interface::ffi::MediaStreamInterface;
        type NativeCreateSdpObserverHandle = crate::jsep::ffi::NativeCreateSdpObserverHandle;
        type NativeSetLocalSdpObserverHandle = crate::jsep::ffi::NativeSetLocalSdpObserverHandle;
        type NativeSetRemoteSdpObserverHandle = crate::jsep::ffi::NativeSetRemoteSdpObserverHandle;
        type SessionDescription = crate::jsep::ffi::SessionDescription;

        type NativePeerConnectionObserver;
        type PeerConnection;

        fn create_offer(
            self: Pin<&mut PeerConnection>,
            observer: UniquePtr<NativeCreateSdpObserverHandle>,
            options: RTCOfferAnswerOptions,
        );
        fn create_answer(
            self: Pin<&mut PeerConnection>,
            observer: UniquePtr<NativeCreateSdpObserverHandle>,
            options: RTCOfferAnswerOptions,
        );
        fn set_local_description(
            self: Pin<&mut PeerConnection>,
            desc: UniquePtr<SessionDescription>,
            observer: UniquePtr<NativeSetLocalSdpObserverHandle>,
        );
        fn set_remote_description(
            self: Pin<&mut PeerConnection>,
            desc: UniquePtr<SessionDescription>,
            observer: UniquePtr<NativeSetRemoteSdpObserverHandle>,
        );
        fn close(self: Pin<&mut PeerConnection>);

        fn create_native_peer_connection_observer(
            observer: Box<PeerConnectionObserverWrapper>,
        ) -> UniquePtr<NativePeerConnectionObserver>;

        fn _unique_peer_connection() -> UniquePtr<PeerConnection>; // Ignore
    }

    extern "Rust" {
        type PeerConnectionObserverWrapper;

        fn on_signaling_change(self: &mut PeerConnectionObserverWrapper, new_state: SignalingState);
        fn on_add_stream(
            self: &mut PeerConnectionObserverWrapper,
            stream: UniquePtr<MediaStreamInterface>,
        );
        fn on_remove_stream(
            self: &mut PeerConnectionObserverWrapper,
            stream: UniquePtr<MediaStreamInterface>,
        );
        fn on_data_channel(
            self: &mut PeerConnectionObserverWrapper,
            data_channel: UniquePtr<DataChannel>,
        );
        fn on_renegotiation_needed(self: &mut PeerConnectionObserverWrapper);
        fn on_negotiation_needed_event(self: &mut PeerConnectionObserverWrapper, event: u32);
        fn on_ice_connection_change(
            self: &mut PeerConnectionObserverWrapper,
            new_state: IceConnectionState,
        );
        fn on_standardized_ice_connection_change(
            self: &mut PeerConnectionObserverWrapper,
            new_state: IceConnectionState,
        );
        fn on_connection_change(
            self: &mut PeerConnectionObserverWrapper,
            new_state: PeerConnectionState,
        );
        fn on_ice_gathering_change(
            self: &mut PeerConnectionObserverWrapper,
            new_state: IceGatheringState,
        );
        fn on_ice_candidate(
            self: &mut PeerConnectionObserverWrapper,
            candidate: UniquePtr<IceCandidate>,
        );
        fn on_ice_candidate_error(
            self: &mut PeerConnectionObserverWrapper,
            address: String,
            port: i32,
            url: String,
            error_code: i32,
            error_text: String,
        );
        fn on_ice_candidates_removed(
            self: &mut PeerConnectionObserverWrapper,
            removed: Vec<CandidatePtr>,
        );
        fn on_ice_connection_receiving_change(
            self: &mut PeerConnectionObserverWrapper,
            receiving: bool,
        );
        fn on_ice_selected_candidate_pair_changed(
            self: &mut PeerConnectionObserverWrapper,
            event: CandidatePairChangeEvent,
        );
        fn on_add_track(
            self: &mut PeerConnectionObserverWrapper,
            receiver: UniquePtr<RtpReceiver>,
            streams: Vec<MediaStreamPtr>,
        );
        fn on_track(
            self: &mut PeerConnectionObserverWrapper,
            transceiver: UniquePtr<RtpTransceiver>,
        );
        fn on_remove_track(
            self: &mut PeerConnectionObserverWrapper,
            receiver: UniquePtr<RtpReceiver>,
        );
        fn on_interesting_usage(self: &mut PeerConnectionObserverWrapper, usage_pattern: i32);
    }
}

// https://webrtc.github.io/webrtc-org/native-code/native-apis/
unsafe impl Sync for ffi::PeerConnection {}
unsafe impl Send for ffi::PeerConnection {}

impl Default for ffi::RTCOfferAnswerOptions {
    /*
       static const int kUndefined = -1;
       static const int kMaxOfferToReceiveMedia = 1;
       static const int kOfferToReceiveMediaTrue = 1;
    */

    fn default() -> Self {
        Self {
            offer_to_receive_video: -1,
            offer_to_receive_audio: -1,
            voice_activity_detection: true,
            ice_restart: false,
            use_rtp_mux: true,
            raw_packetization_for_video: false,
            num_simulcast_layers: 1,
            use_obsolete_sctp_sdp: false,
        }
    }
}

pub trait PeerConnectionObserver: Send + Sync {
    fn on_signaling_change(&mut self, new_state: ffi::SignalingState);
    fn on_add_stream(&mut self, stream: UniquePtr<MediaStreamInterface>);
    fn on_remove_stream(&mut self, stream: UniquePtr<MediaStreamInterface>);
    fn on_data_channel(&mut self, data_channel: UniquePtr<DataChannel>);
    fn on_renegotiation_needed(&mut self);
    fn on_negotiation_needed_event(&mut self, event: u32);
    fn on_ice_connection_change(&mut self, new_state: ffi::IceConnectionState);
    fn on_standardized_ice_connection_change(&mut self, new_state: ffi::IceConnectionState);
    fn on_connection_change(&mut self, new_state: ffi::PeerConnectionState);
    fn on_ice_gathering_change(&mut self, new_state: ffi::IceGatheringState);
    fn on_ice_candidate(&mut self, candidate: UniquePtr<IceCandidate>);
    fn on_ice_candidate_error(
        &mut self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    );
    fn on_ice_candidates_removed(&mut self, removed: Vec<UniquePtr<Candidate>>);
    fn on_ice_connection_receiving_change(&mut self, receiving: bool);
    fn on_ice_selected_candidate_pair_changed(&mut self, event: ffi::CandidatePairChangeEvent);
    fn on_add_track(
        &mut self,
        receiver: UniquePtr<RtpReceiver>,
        streams: Vec<UniquePtr<MediaStreamInterface>>,
    );
    fn on_track(&mut self, transceiver: UniquePtr<RtpTransceiver>);
    fn on_remove_track(&mut self, receiver: UniquePtr<RtpReceiver>);
    fn on_interesting_usage(&mut self, usage_pattern: i32);
}

pub struct PeerConnectionObserverWrapper {
    observer: Rc<RefCell<dyn PeerConnectionObserver>>,
}

impl PeerConnectionObserverWrapper {
    pub fn new(observer: Rc<RefCell<dyn PeerConnectionObserver>>) -> Self {
        Self { observer }
    }

    fn on_signaling_change(&mut self, new_state: ffi::SignalingState) {
        self.observer.borrow_mut().on_signaling_change(new_state);
    }

    fn on_add_stream(&mut self, stream: UniquePtr<MediaStreamInterface>) {
        self.observer.borrow_mut().on_add_stream(stream);
    }

    fn on_remove_stream(&mut self, stream: UniquePtr<MediaStreamInterface>) {
        self.observer.borrow_mut().on_remove_stream(stream);
    }

    fn on_data_channel(&mut self, data_channel: UniquePtr<DataChannel>) {
        self.observer.borrow_mut().on_data_channel(data_channel);
    }

    fn on_renegotiation_needed(&mut self) {
        self.observer.borrow_mut().on_renegotiation_needed();
    }

    fn on_negotiation_needed_event(&mut self, event: u32) {
        self.observer.borrow_mut().on_negotiation_needed_event(event);
    }

    fn on_ice_connection_change(&mut self, new_state: ffi::IceConnectionState) {
        self.observer.borrow_mut().on_ice_connection_change(new_state);
    }

    fn on_standardized_ice_connection_change(&mut self, new_state: ffi::IceConnectionState) {
        self.observer.borrow_mut().on_standardized_ice_connection_change(new_state);
    }

    fn on_connection_change(&mut self, new_state: ffi::PeerConnectionState) {
        self.observer.borrow_mut().on_connection_change(new_state);
    }

    fn on_ice_gathering_change(&mut self, new_state: ffi::IceGatheringState) {
        self.observer.borrow_mut().on_ice_gathering_change(new_state);
    }

    fn on_ice_candidate(&mut self, candidate: UniquePtr<IceCandidate>) {
        self.observer.borrow_mut().on_ice_candidate(candidate);
    }

    fn on_ice_candidate_error(
        &mut self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    ) {
        self.observer
            .borrow_mut().on_ice_candidate_error(address, port, url, error_code, error_text);
    }

    fn on_ice_candidates_removed(&mut self, removed: Vec<ffi::CandidatePtr>) {
        let mut vec = Vec::new();

        for v in removed {
            vec.push(v.ptr);
        }

        self.observer.borrow_mut().on_ice_candidates_removed(vec);
    }

    fn on_ice_connection_receiving_change(&mut self, receiving: bool) {
        self.observer.borrow_mut().on_ice_connection_receiving_change(receiving);
    }

    fn on_ice_selected_candidate_pair_changed(&mut self, event: ffi::CandidatePairChangeEvent) {
        self.observer.borrow_mut().on_ice_selected_candidate_pair_changed(event);
    }

    fn on_add_track(
        &mut self,
        receiver: UniquePtr<RtpReceiver>,
        streams: Vec<ffi::MediaStreamPtr>,
    ) {
        let mut vec = Vec::new();

        for v in streams {
            vec.push(v.ptr);
        }

        self.observer.borrow_mut().on_add_track(receiver, vec);
    }

    fn on_track(&mut self, transceiver: UniquePtr<RtpTransceiver>) {
        self.observer.borrow_mut().on_track(transceiver);
    }

    fn on_remove_track(&mut self, receiver: UniquePtr<RtpReceiver>) {
        self.observer.borrow_mut().on_remove_track(receiver);
    }

    fn on_interesting_usage(&mut self, usage_pattern: i32) {
        self.observer.borrow_mut().on_interesting_usage(usage_pattern);
    }
}
