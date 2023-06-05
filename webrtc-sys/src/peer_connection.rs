use crate::candidate::ffi::Candidate;
use crate::data_channel::ffi::DataChannel;
use crate::impl_thread_safety;
use crate::jsep::ffi::IceCandidate;
use crate::media_stream::ffi::MediaStream;
use crate::rtp_receiver::ffi::RtpReceiver;
use crate::rtp_transceiver::ffi::RtpTransceiver;
use cxx::SharedPtr;
use std::any::Any;
use std::sync::Arc;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    pub struct CandidatePair {
        local: SharedPtr<Candidate>,
        remote: SharedPtr<Candidate>,
    }

    pub struct CandidatePairChangeEvent {
        selected_candidate_pair: CandidatePair,
        last_data_received_ms: i64,
        reason: String,
        estimated_disconnected_time_ms: i64,
    }

    #[repr(i32)]
    pub enum PeerConnectionState {
        New,
        Connecting,
        Connected,
        Disconnected,
        Failed,
        Closed,
    }

    #[repr(i32)]
    pub enum SignalingState {
        Stable,
        HaveLocalOffer,
        HaveLocalPrAnswer,
        HaveRemoteOffer,
        HaveRemotePrAnswer,
        Closed,
    }

    #[repr(i32)]
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

    #[repr(i32)]
    pub enum IceGatheringState {
        IceGatheringNew,
        IceGatheringGathering,
        IceGatheringComplete,
    }

    pub struct RtcOfferAnswerOptions {
        offer_to_receive_video: i32,
        offer_to_receive_audio: i32,
        voice_activity_detection: bool,
        ice_restart: bool,
        use_rtp_mux: bool,
        raw_packetization_for_video: bool,
        num_simulcast_layers: i32,
        use_obsolete_sctp_sdp: bool,
    }

    extern "C++" {
        include!("livekit/rtc_error.h");
        include!("livekit/helper.h");
        include!("livekit/candidate.h");
        include!("livekit/media_stream.h");
        include!("livekit/rtp_transceiver.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/data_channel.h");
        include!("livekit/jsep.h");
        include!("livekit/webrtc.h");

        type MediaStreamPtr = crate::helper::ffi::MediaStreamPtr;
        type CandidatePtr = crate::helper::ffi::CandidatePtr;
        type RtpSenderPtr = crate::helper::ffi::RtpSenderPtr;
        type RtpReceiverPtr = crate::helper::ffi::RtpReceiverPtr;
        type RtpTransceiverPtr = crate::helper::ffi::RtpTransceiverPtr;
        type RtcError = crate::rtc_error::ffi::RtcError;
        type Candidate = crate::candidate::ffi::Candidate;
        type IceCandidate = crate::jsep::ffi::IceCandidate;
        type DataChannel = crate::data_channel::ffi::DataChannel;
        type DataChannelInit = crate::data_channel::ffi::DataChannelInit;
        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type RtpTransceiver = crate::rtp_transceiver::ffi::RtpTransceiver;
        type RtpTransceiverInit = crate::rtp_transceiver::ffi::RtpTransceiverInit;
        type MediaStream = crate::media_stream::ffi::MediaStream;
        type MediaStreamTrack = crate::media_stream::ffi::MediaStreamTrack;
        type SessionDescription = crate::jsep::ffi::SessionDescription;
        type MediaType = crate::webrtc::ffi::MediaType;
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection.h");

        type PeerConnection;

        // The reason we still expose NativePeerConnectionObserver is because cxx doeesn't support Rust type alias
        // So we can't share NativePeerConnectionWrapper in peer_connection_factory.rs
        // (It is technically possible to get the Opaque C++ Type, but in this case, we can't use Box<T>)
        // We can delete create_native_peer_connection_observer once cxx supports Rust type alias
        type NativePeerConnectionObserver;
        fn create_native_peer_connection_observer(
            observer: Box<PeerConnectionObserverWrapper>,
        ) -> UniquePtr<NativePeerConnectionObserver>;

        fn create_offer(
            self: &PeerConnection,
            options: RtcOfferAnswerOptions,
            ctx: Box<AsyncContext>,
            on_success: fn(ctx: Box<AsyncContext>, sdp: UniquePtr<SessionDescription>),
            on_error: fn(ctx: Box<AsyncContext>, error: RtcError),
        );
        fn create_answer(
            self: &PeerConnection,
            options: RtcOfferAnswerOptions,
            ctx: Box<AsyncContext>,
            on_success: fn(ctx: Box<AsyncContext>, sdp: UniquePtr<SessionDescription>),
            on_error: fn(ctx: Box<AsyncContext>, error: RtcError),
        );
        fn set_local_description(
            self: &PeerConnection,
            desc: UniquePtr<SessionDescription>,
            ctx: Box<AsyncContext>,
            on_complete: fn(ctx: Box<AsyncContext>, error: RtcError),
        );
        fn set_remote_description(
            self: &PeerConnection,
            desc: UniquePtr<SessionDescription>,
            ctx: Box<AsyncContext>,
            on_complete: fn(ctx: Box<AsyncContext>, error: RtcError),
        );
        fn add_track(
            self: &PeerConnection,
            track: SharedPtr<MediaStreamTrack>,
            stream_ids: &Vec<String>,
        ) -> Result<SharedPtr<RtpSender>>;
        fn remove_track(self: &PeerConnection, sender: SharedPtr<RtpSender>) -> Result<()>;
        fn add_transceiver(
            self: &PeerConnection,
            track: SharedPtr<MediaStreamTrack>,
            init: RtpTransceiverInit,
        ) -> Result<SharedPtr<RtpTransceiver>>;
        fn add_transceiver_for_media(
            self: &PeerConnection,
            media_type: MediaType,
            init: RtpTransceiverInit,
        ) -> Result<SharedPtr<RtpTransceiver>>;
        fn get_senders(self: &PeerConnection) -> Vec<RtpSenderPtr>;
        fn get_receivers(self: &PeerConnection) -> Vec<RtpReceiverPtr>;
        fn get_transceivers(self: &PeerConnection) -> Vec<RtpTransceiverPtr>;
        fn create_data_channel(
            self: &PeerConnection,
            label: String,
            init: DataChannelInit,
        ) -> Result<SharedPtr<DataChannel>>;
        fn add_ice_candidate(
            self: &PeerConnection,
            candidate: SharedPtr<IceCandidate>,
            ctx: Box<AsyncContext>,
            on_complete: fn(ctx: Box<AsyncContext>, error: RtcError),
        );
        fn current_local_description(self: &PeerConnection) -> UniquePtr<SessionDescription>;
        fn current_remote_description(self: &PeerConnection) -> UniquePtr<SessionDescription>;
        fn connection_state(self: &PeerConnection) -> PeerConnectionState;
        fn signaling_state(self: &PeerConnection) -> SignalingState;
        fn ice_gathering_state(self: &PeerConnection) -> IceGatheringState;
        fn ice_connection_state(self: &PeerConnection) -> IceConnectionState;
        fn close(self: &PeerConnection);

        fn _shared_peer_connection() -> SharedPtr<PeerConnection>; // Ignore
    }

    extern "Rust" {
        type AsyncContext;
        type PeerConnectionObserverWrapper;

        fn on_signaling_change(self: &PeerConnectionObserverWrapper, new_state: SignalingState);
        fn on_add_stream(self: &PeerConnectionObserverWrapper, stream: SharedPtr<MediaStream>);
        fn on_remove_stream(self: &PeerConnectionObserverWrapper, stream: SharedPtr<MediaStream>);
        fn on_data_channel(
            self: &PeerConnectionObserverWrapper,
            data_channel: SharedPtr<DataChannel>,
        );
        fn on_renegotiation_needed(self: &PeerConnectionObserverWrapper);
        fn on_negotiation_needed_event(self: &PeerConnectionObserverWrapper, event: u32);
        fn on_ice_connection_change(
            self: &PeerConnectionObserverWrapper,
            new_state: IceConnectionState,
        );
        fn on_standardized_ice_connection_change(
            self: &PeerConnectionObserverWrapper,
            new_state: IceConnectionState,
        );
        fn on_connection_change(
            self: &PeerConnectionObserverWrapper,
            new_state: PeerConnectionState,
        );
        fn on_ice_gathering_change(
            self: &PeerConnectionObserverWrapper,
            new_state: IceGatheringState,
        );
        fn on_ice_candidate(
            self: &PeerConnectionObserverWrapper,
            candidate: SharedPtr<IceCandidate>,
        );
        fn on_ice_candidate_error(
            self: &PeerConnectionObserverWrapper,
            address: String,
            port: i32,
            url: String,
            error_code: i32,
            error_text: String,
        );
        fn on_ice_candidates_removed(
            self: &PeerConnectionObserverWrapper,
            removed: Vec<CandidatePtr>,
        );
        fn on_ice_connection_receiving_change(
            self: &PeerConnectionObserverWrapper,
            receiving: bool,
        );
        fn on_ice_selected_candidate_pair_changed(
            self: &PeerConnectionObserverWrapper,
            event: CandidatePairChangeEvent,
        );
        fn on_add_track(
            self: &PeerConnectionObserverWrapper,
            receiver: SharedPtr<RtpReceiver>,
            streams: Vec<MediaStreamPtr>,
        );
        fn on_track(self: &PeerConnectionObserverWrapper, transceiver: SharedPtr<RtpTransceiver>);
        fn on_remove_track(self: &PeerConnectionObserverWrapper, receiver: SharedPtr<RtpReceiver>);
        fn on_interesting_usage(self: &PeerConnectionObserverWrapper, usage_pattern: i32);
    }
}

#[repr(transparent)]
pub struct AsyncContext(pub Box<dyn Any + Send>);

// https://webrtc.github.io/webrtc-org/native-code/native-apis/
impl_thread_safety!(ffi::PeerConnection, Send + Sync);

impl Default for ffi::RtcOfferAnswerOptions {
    // static const int kUndefined = -1;
    // static const int kMaxOfferToReceiveMedia = 1;
    // static const int kOfferToReceiveMediaTrue = 1;

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
    fn on_signaling_change(&self, new_state: ffi::SignalingState);
    fn on_add_stream(&self, stream: SharedPtr<MediaStream>);
    fn on_remove_stream(&self, stream: SharedPtr<MediaStream>);
    fn on_data_channel(&self, data_channel: SharedPtr<DataChannel>);
    fn on_renegotiation_needed(&self);
    fn on_negotiation_needed_event(&self, event: u32);
    fn on_ice_connection_change(&self, new_state: ffi::IceConnectionState);
    fn on_standardized_ice_connection_change(&self, new_state: ffi::IceConnectionState);
    fn on_connection_change(&self, new_state: ffi::PeerConnectionState);
    fn on_ice_gathering_change(&self, new_state: ffi::IceGatheringState);
    fn on_ice_candidate(&self, candidate: SharedPtr<IceCandidate>);
    fn on_ice_candidate_error(
        &self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    );
    fn on_ice_candidates_removed(&self, removed: Vec<SharedPtr<Candidate>>);
    fn on_ice_connection_receiving_change(&self, receiving: bool);
    fn on_ice_selected_candidate_pair_changed(&self, event: ffi::CandidatePairChangeEvent);
    fn on_add_track(&self, receiver: SharedPtr<RtpReceiver>, streams: Vec<SharedPtr<MediaStream>>);
    fn on_track(&self, transceiver: SharedPtr<RtpTransceiver>);
    fn on_remove_track(&self, receiver: SharedPtr<RtpReceiver>);
    fn on_interesting_usage(&self, usage_pattern: i32);
}

// Wrapper for PeerConnectionObserver because cxx doesn't support dyn Trait on c++
// https://github.com/dtolnay/cxx/issues/665
pub struct PeerConnectionObserverWrapper {
    observer: Arc<dyn PeerConnectionObserver>,
}

impl PeerConnectionObserverWrapper {
    pub fn new(observer: Arc<dyn PeerConnectionObserver>) -> Self {
        Self { observer }
    }

    fn on_signaling_change(&self, new_state: ffi::SignalingState) {
        self.observer.on_signaling_change(new_state);
    }

    fn on_add_stream(&self, stream: SharedPtr<MediaStream>) {
        self.observer.on_add_stream(stream);
    }

    fn on_remove_stream(&self, stream: SharedPtr<MediaStream>) {
        self.observer.on_remove_stream(stream);
    }

    fn on_data_channel(&self, data_channel: SharedPtr<DataChannel>) {
        self.observer.on_data_channel(data_channel);
    }

    fn on_renegotiation_needed(&self) {
        self.observer.on_renegotiation_needed();
    }

    fn on_negotiation_needed_event(&self, event: u32) {
        self.observer.on_negotiation_needed_event(event);
    }

    fn on_ice_connection_change(&self, new_state: ffi::IceConnectionState) {
        self.observer.on_ice_connection_change(new_state);
    }

    fn on_standardized_ice_connection_change(&self, new_state: ffi::IceConnectionState) {
        self.observer
            .on_standardized_ice_connection_change(new_state);
    }

    fn on_connection_change(&self, new_state: ffi::PeerConnectionState) {
        self.observer.on_connection_change(new_state);
    }

    fn on_ice_gathering_change(&self, new_state: ffi::IceGatheringState) {
        self.observer.on_ice_gathering_change(new_state);
    }

    fn on_ice_candidate(&self, candidate: SharedPtr<IceCandidate>) {
        self.observer.on_ice_candidate(candidate);
    }

    fn on_ice_candidate_error(
        &self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    ) {
        self.observer
            .on_ice_candidate_error(address, port, url, error_code, error_text);
    }

    fn on_ice_candidates_removed(&self, candidates: Vec<ffi::CandidatePtr>) {
        self.observer
            .on_ice_candidates_removed(candidates.into_iter().map(|v| v.ptr).collect());
    }

    fn on_ice_connection_receiving_change(&self, receiving: bool) {
        self.observer.on_ice_connection_receiving_change(receiving);
    }

    fn on_ice_selected_candidate_pair_changed(&self, event: ffi::CandidatePairChangeEvent) {
        self.observer.on_ice_selected_candidate_pair_changed(event);
    }

    fn on_add_track(&self, receiver: SharedPtr<RtpReceiver>, streams: Vec<ffi::MediaStreamPtr>) {
        self.observer
            .on_add_track(receiver, streams.into_iter().map(|v| v.ptr).collect());
    }

    fn on_track(&self, transceiver: SharedPtr<RtpTransceiver>) {
        self.observer.on_track(transceiver);
    }

    fn on_remove_track(&self, receiver: SharedPtr<RtpReceiver>) {
        self.observer.on_remove_track(receiver);
    }

    fn on_interesting_usage(&self, usage_pattern: i32) {
        self.observer.on_interesting_usage(usage_pattern);
    }
}
