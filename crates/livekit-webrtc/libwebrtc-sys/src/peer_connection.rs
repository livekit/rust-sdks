use std::fmt::{Debug, Formatter};
use cxx::UniquePtr;

use crate::candidate::ffi::Candidate;
use crate::data_channel::ffi::DataChannel;
use crate::jsep::ffi::IceCandidate;
use crate::media_stream_interface::ffi::MediaStreamInterface;
use crate::rtc_error::ffi::RTCError;
use crate::rtp_receiver::ffi::RtpReceiver;
use crate::rtp_transceiver::ffi::RtpTransceiver;

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
    #[repr(i32)]
    pub enum PeerConnectionState {
        New,
        Connecting,
        Connected,
        Disconnected,
        Failed,
        Closed,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum SignalingState {
        Stable,
        HaveLocalOffer,
        HaveLocalPrAnswer,
        HaveRemoteOffer,
        HaveRemotePrAnswer,
        Closed,
    }

    #[derive(Debug)]
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

    #[derive(Debug)]
    #[repr(i32)]
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
        include!("libwebrtc-sys/src/rtc_error.rs.h");

        type RTCError = crate::rtc_error::ffi::RTCError;
        type Candidate = crate::candidate::ffi::Candidate;
        type IceCandidate = crate::jsep::ffi::IceCandidate;
        type DataChannel = crate::data_channel::ffi::DataChannel;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type RtpTransceiver = crate::rtp_transceiver::ffi::RtpTransceiver;
        type MediaStreamInterface = crate::media_stream_interface::ffi::MediaStreamInterface;
        type NativeCreateSdpObserverHandle = crate::jsep::ffi::NativeCreateSdpObserverHandle;
        type NativeSetLocalSdpObserverHandle = crate::jsep::ffi::NativeSetLocalSdpObserverHandle;
        type NativeSetRemoteSdpObserverHandle = crate::jsep::ffi::NativeSetRemoteSdpObserverHandle;
        type NativeDataChannelInit = crate::data_channel::ffi::NativeDataChannelInit;
        type SessionDescription = crate::jsep::ffi::SessionDescription;
        type RTCRuntime = crate::webrtc::ffi::RTCRuntime;

        type NativeAddIceCandidateObserver;
        type NativePeerConnectionObserver;
        type PeerConnection;

        /// SAFETY
        /// The observer must live as long as the operation ends
        unsafe fn create_offer(
            self: Pin<&mut PeerConnection>,
            observer: Pin<&mut NativeCreateSdpObserverHandle>,
            options: RTCOfferAnswerOptions,
        );

        /// SAFETY
        /// The observer must live as long as the operation ends
        unsafe fn create_answer(
            self: Pin<&mut PeerConnection>,
            observer: Pin<&mut NativeCreateSdpObserverHandle>,
            options: RTCOfferAnswerOptions,
        );

        /// SAFETY
        /// The observer must live as long as the operation ends
        unsafe fn set_local_description(
            self: Pin<&mut PeerConnection>,
            desc: UniquePtr<SessionDescription>,
            observer: Pin<&mut NativeSetLocalSdpObserverHandle>,
        );

        /// SAFETY
        /// The observer must live as long as the operation ends
        unsafe fn set_remote_description(
            self: Pin<&mut PeerConnection>,
            desc: UniquePtr<SessionDescription>,
            observer: Pin<&mut NativeSetRemoteSdpObserverHandle>,
        );

        fn create_data_channel(
            self: Pin<&mut PeerConnection>,
            label: String,
            init: UniquePtr<NativeDataChannelInit>,
        ) -> Result<UniquePtr<DataChannel>>;

        fn add_ice_candidate(
            self: Pin<&mut PeerConnection>,
            candidate: UniquePtr<IceCandidate>,
            observer: Pin<&mut NativeAddIceCandidateObserver>,
        );

        fn local_description(self: &PeerConnection) -> UniquePtr<SessionDescription>;

        fn remote_description(self: &PeerConnection) -> UniquePtr<SessionDescription>;

        fn signaling_state(self: &PeerConnection) -> SignalingState;

        fn ice_gathering_state(self: &PeerConnection) -> IceGatheringState;

        fn ice_connection_state(self: &PeerConnection) -> IceConnectionState;

        fn close(self: Pin<&mut PeerConnection>);

        fn create_native_peer_connection_observer(
            rtc_runtime: SharedPtr<RTCRuntime>,
            observer: Box<PeerConnectionObserverWrapper>,
        ) -> UniquePtr<NativePeerConnectionObserver>;

        fn create_native_add_ice_candidate_observer(
            observer: Box<AddIceCandidateObserverWrapper>,
        ) -> UniquePtr<NativeAddIceCandidateObserver>;

        fn _unique_peer_connection() -> UniquePtr<PeerConnection>; // Ignore
    }

    extern "Rust" {
        type AddIceCandidateObserverWrapper;

        fn on_complete(self: &AddIceCandidateObserverWrapper, error: RTCError);

        type PeerConnectionObserverWrapper;

        fn on_signaling_change(self: &PeerConnectionObserverWrapper, new_state: SignalingState);
        fn on_add_stream(
            self: &PeerConnectionObserverWrapper,
            stream: UniquePtr<MediaStreamInterface>,
        );
        fn on_remove_stream(
            self: &PeerConnectionObserverWrapper,
            stream: UniquePtr<MediaStreamInterface>,
        );
        fn on_data_channel(
            self: &PeerConnectionObserverWrapper,
            data_channel: UniquePtr<DataChannel>,
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
            candidate: UniquePtr<IceCandidate>,
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
            receiver: UniquePtr<RtpReceiver>,
            streams: Vec<MediaStreamPtr>,
        );
        fn on_track(self: &PeerConnectionObserverWrapper, transceiver: UniquePtr<RtpTransceiver>);
        fn on_remove_track(self: &PeerConnectionObserverWrapper, receiver: UniquePtr<RtpReceiver>);
        fn on_interesting_usage(self: &PeerConnectionObserverWrapper, usage_pattern: i32);
    }
}

// https://webrtc.github.io/webrtc-org/native-code/native-apis/
unsafe impl Send for ffi::PeerConnection {}
unsafe impl Sync for ffi::PeerConnection {}

unsafe impl Send for ffi::NativePeerConnectionObserver {}
unsafe impl Sync for ffi::NativePeerConnectionObserver {}

unsafe impl Sync for ffi::NativeAddIceCandidateObserver {}
unsafe impl Send for ffi::NativeAddIceCandidateObserver {}

unsafe impl Sync for ffi::NativeSetRemoteSdpObserverHandle {}
unsafe impl Send for ffi::NativeSetRemoteSdpObserverHandle {}

unsafe impl Sync for ffi::NativeSetLocalSdpObserverHandle {}
unsafe impl Send for ffi::NativeSetLocalSdpObserverHandle {}

unsafe impl Sync for ffi::NativeCreateSdpObserverHandle {}
unsafe impl Send for ffi::NativeCreateSdpObserverHandle {}

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

pub trait AddIceCandidateObserver: Send {
    fn on_complete(&self, error: RTCError);
}

pub struct AddIceCandidateObserverWrapper {
    observer: Box<dyn AddIceCandidateObserver>,
}

impl AddIceCandidateObserverWrapper {
    pub fn new(observer: Box<dyn AddIceCandidateObserver>) -> Self {
        Self { observer }
    }

    fn on_complete(&self, error: RTCError) {
        self.observer.on_complete(error);
    }
}

pub trait PeerConnectionObserver: Send + Sync {
    fn on_signaling_change(&self, new_state: ffi::SignalingState);
    fn on_add_stream(&self, stream: UniquePtr<MediaStreamInterface>);
    fn on_remove_stream(&self, stream: UniquePtr<MediaStreamInterface>);
    fn on_data_channel(&self, data_channel: UniquePtr<DataChannel>);
    fn on_renegotiation_needed(&self);
    fn on_negotiation_needed_event(&self, event: u32);
    fn on_ice_connection_change(&self, new_state: ffi::IceConnectionState);
    fn on_standardized_ice_connection_change(&self, new_state: ffi::IceConnectionState);
    fn on_connection_change(&self, new_state: ffi::PeerConnectionState);
    fn on_ice_gathering_change(&self, new_state: ffi::IceGatheringState);
    fn on_ice_candidate(&self, candidate: UniquePtr<IceCandidate>);
    fn on_ice_candidate_error(
        &self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    );
    fn on_ice_candidates_removed(&self, removed: Vec<UniquePtr<Candidate>>);
    fn on_ice_connection_receiving_change(&self, receiving: bool);
    fn on_ice_selected_candidate_pair_changed(&self, event: ffi::CandidatePairChangeEvent);
    fn on_add_track(
        &self,
        receiver: UniquePtr<RtpReceiver>,
        streams: Vec<UniquePtr<MediaStreamInterface>>,
    );
    fn on_track(&self, transceiver: UniquePtr<RtpTransceiver>);
    fn on_remove_track(&self, receiver: UniquePtr<RtpReceiver>);
    fn on_interesting_usage(&self, usage_pattern: i32);
}

// Thread safety is handled inside PeerConnectionObserver
pub struct PeerConnectionObserverWrapper {
    observer: *mut dyn PeerConnectionObserver,
}

impl PeerConnectionObserverWrapper {
    /// SAFETY
    /// PeerConnectionObserver must lives as long as PeerConnectionObserverWrapper does
    pub unsafe fn new(observer: *mut dyn PeerConnectionObserver) -> Self {
        Self { observer }
    }

    fn on_signaling_change(&self, new_state: ffi::SignalingState) {
        unsafe {
            (*self.observer).on_signaling_change(new_state);
        }
    }

    fn on_add_stream(&self, stream: UniquePtr<MediaStreamInterface>) {
        unsafe {
            (*self.observer).on_add_stream(stream);
        }
    }

    fn on_remove_stream(&self, stream: UniquePtr<MediaStreamInterface>) {
        unsafe {
            (*self.observer).on_remove_stream(stream);
        }
    }

    fn on_data_channel(&self, data_channel: UniquePtr<DataChannel>) {
        unsafe {
            (*self.observer).on_data_channel(data_channel);
        }
    }

    fn on_renegotiation_needed(&self) {
        unsafe {
            (*self.observer).on_renegotiation_needed();
        }
    }

    fn on_negotiation_needed_event(&self, event: u32) {
        unsafe {
            (*self.observer).on_negotiation_needed_event(event);
        }
    }

    fn on_ice_connection_change(&self, new_state: ffi::IceConnectionState) {
        unsafe {
            (*self.observer).on_ice_connection_change(new_state);
        }
    }

    fn on_standardized_ice_connection_change(&self, new_state: ffi::IceConnectionState) {
        unsafe {
            (*self.observer).on_standardized_ice_connection_change(new_state);
        }
    }

    fn on_connection_change(&self, new_state: ffi::PeerConnectionState) {
        unsafe {
            (*self.observer).on_connection_change(new_state);
        }
    }

    fn on_ice_gathering_change(&self, new_state: ffi::IceGatheringState) {
        unsafe {
            (*self.observer).on_ice_gathering_change(new_state);
        }
    }

    fn on_ice_candidate(&self, candidate: UniquePtr<IceCandidate>) {
        unsafe {
            (*self.observer).on_ice_candidate(candidate);
        }
    }

    fn on_ice_candidate_error(
        &self,
        address: String,
        port: i32,
        url: String,
        error_code: i32,
        error_text: String,
    ) {
        unsafe {
            (*self.observer).on_ice_candidate_error(address, port, url, error_code, error_text);
        }
    }

    fn on_ice_candidates_removed(&self, removed: Vec<ffi::CandidatePtr>) {
        let mut vec = Vec::new();

        for v in removed {
            vec.push(v.ptr);
        }

        unsafe {
            (*self.observer).on_ice_candidates_removed(vec);
        }
    }

    fn on_ice_connection_receiving_change(&self, receiving: bool) {
        unsafe {
            (*self.observer).on_ice_connection_receiving_change(receiving);
        }
    }

    fn on_ice_selected_candidate_pair_changed(&self, event: ffi::CandidatePairChangeEvent) {
        unsafe {
            (*self.observer).on_ice_selected_candidate_pair_changed(event);
        }
    }

    fn on_add_track(&self, receiver: UniquePtr<RtpReceiver>, streams: Vec<ffi::MediaStreamPtr>) {
        let mut vec = Vec::new();

        for v in streams {
            vec.push(v.ptr);
        }

        unsafe {
            (*self.observer).on_add_track(receiver, vec);
        }
    }

    fn on_track(&self, transceiver: UniquePtr<RtpTransceiver>) {
        unsafe {
            (*self.observer).on_track(transceiver);
        }
    }

    fn on_remove_track(&self, receiver: UniquePtr<RtpReceiver>) {
        unsafe {
            (*self.observer).on_remove_track(receiver);
        }
    }

    fn on_interesting_usage(&self, usage_pattern: i32) {
        unsafe {
            (*self.observer).on_interesting_usage(usage_pattern);
        }
    }
}
