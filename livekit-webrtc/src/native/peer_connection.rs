use crate::data_channel::DataChannel;
use crate::data_channel::DataChannelInit;
use crate::ice_candidate::IceCandidate;
use crate::imp::data_channel as imp_dc;
use crate::imp::ice_candidate as imp_ic;
use crate::imp::media_stream as imp_ms;
use crate::imp::rtp_receiver as imp_rr;
use crate::imp::rtp_sender as imp_rs;
use crate::imp::rtp_transceiver as imp_rt;
use crate::imp::session_description as imp_sdp;
use crate::media_stream::{MediaStream, MediaStreamTrack};
use crate::peer_connection::{
    AnswerOptions, IceCandidateError, IceConnectionState, IceGatheringState, OfferOptions,
    OnConnectionChange, OnDataChannel, OnIceCandidate, OnIceCandidateError, OnIceConnectionChange,
    OnIceGatheringChange, OnNegotiationNeeded, OnSignalingChange, OnTrack, PeerConnectionState,
    SignalingState, TrackEvent,
};
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_sender::RtpSender;
use crate::rtp_transceiver::RtpTransceiver;
use crate::rtp_transceiver::RtpTransceiverInit;
use crate::MediaType;
use crate::{session_description::SessionDescription, RtcError};
use cxx::{SharedPtr, UniquePtr};
use futures::channel::oneshot;
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use webrtc_sys::data_channel as sys_dc;
use webrtc_sys::jsep as sys_jsep;
use webrtc_sys::peer_connection as sys_pc;
use webrtc_sys::rtc_error as sys_err;

impl From<OfferOptions> for sys_pc::ffi::RTCOfferAnswerOptions {
    fn from(options: OfferOptions) -> Self {
        Self {
            ice_restart: options.ice_restart,
            offer_to_receive_audio: options.offer_to_receive_audio as i32,
            offer_to_receive_video: options.offer_to_receive_video as i32,
            ..Default::default()
        }
    }
}

impl From<AnswerOptions> for sys_pc::ffi::RTCOfferAnswerOptions {
    fn from(_options: AnswerOptions) -> Self {
        Self::default()
    }
}

impl From<sys_pc::ffi::PeerConnectionState> for PeerConnectionState {
    fn from(state: sys_pc::ffi::PeerConnectionState) -> Self {
        match state {
            sys_pc::ffi::PeerConnectionState::New => PeerConnectionState::New,
            sys_pc::ffi::PeerConnectionState::Connecting => PeerConnectionState::Connecting,
            sys_pc::ffi::PeerConnectionState::Connected => PeerConnectionState::Connected,
            sys_pc::ffi::PeerConnectionState::Disconnected => PeerConnectionState::Disconnected,
            sys_pc::ffi::PeerConnectionState::Failed => PeerConnectionState::Failed,
            sys_pc::ffi::PeerConnectionState::Closed => PeerConnectionState::Closed,
            _ => panic!("unknown PeerConnectionState"),
        }
    }
}

impl From<sys_pc::ffi::IceConnectionState> for IceConnectionState {
    fn from(state: sys_pc::ffi::IceConnectionState) -> Self {
        match state {
            sys_pc::ffi::IceConnectionState::IceConnectionNew => IceConnectionState::New,
            sys_pc::ffi::IceConnectionState::IceConnectionChecking => IceConnectionState::Checking,
            sys_pc::ffi::IceConnectionState::IceConnectionConnected => {
                IceConnectionState::Connected
            }
            sys_pc::ffi::IceConnectionState::IceConnectionCompleted => {
                IceConnectionState::Completed
            }
            sys_pc::ffi::IceConnectionState::IceConnectionFailed => IceConnectionState::Failed,
            sys_pc::ffi::IceConnectionState::IceConnectionDisconnected => {
                IceConnectionState::Disconnected
            }
            sys_pc::ffi::IceConnectionState::IceConnectionClosed => IceConnectionState::Closed,
            sys_pc::ffi::IceConnectionState::IceConnectionMax => IceConnectionState::Max,
            _ => panic!("unknown IceConnectionState"),
        }
    }
}

impl From<sys_pc::ffi::IceGatheringState> for IceGatheringState {
    fn from(state: sys_pc::ffi::IceGatheringState) -> Self {
        match state {
            sys_pc::ffi::IceGatheringState::IceGatheringNew => IceGatheringState::New,
            sys_pc::ffi::IceGatheringState::IceGatheringGathering => IceGatheringState::Gathering,
            sys_pc::ffi::IceGatheringState::IceGatheringComplete => IceGatheringState::Complete,
            _ => panic!("unknown IceGatheringState"),
        }
    }
}

impl From<sys_pc::ffi::SignalingState> for SignalingState {
    fn from(state: sys_pc::ffi::SignalingState) -> Self {
        match state {
            sys_pc::ffi::SignalingState::Stable => SignalingState::Stable,
            sys_pc::ffi::SignalingState::HaveLocalOffer => SignalingState::HaveLocalOffer,
            sys_pc::ffi::SignalingState::HaveRemoteOffer => SignalingState::HaveRemoteOffer,
            sys_pc::ffi::SignalingState::HaveLocalPrAnswer => SignalingState::HaveLocalPrAnswer,
            sys_pc::ffi::SignalingState::HaveRemotePrAnswer => SignalingState::HaveRemotePrAnswer,
            sys_pc::ffi::SignalingState::Closed => SignalingState::Closed,
            _ => panic!("unknown SignalingState"),
        }
    }
}

#[derive(Clone)]
pub struct PeerConnection {
    native_observer: SharedPtr<sys_pc::ffi::NativePeerConnectionObserver>,
    observer: Arc<PeerObserver>,

    pub(crate) sys_handle: SharedPtr<sys_pc::ffi::PeerConnection>,
}

impl PeerConnection {
    pub fn configure(
        sys_handle: SharedPtr<sys_pc::ffi::PeerConnection>,
        observer: Arc<PeerObserver>,
        native_observer: SharedPtr<sys_pc::ffi::NativePeerConnectionObserver>,
    ) -> Self {
        Self {
            sys_handle,
            observer,
            native_observer,
        }
    }

    pub async fn create_offer(
        &self,
        options: OfferOptions,
    ) -> Result<SessionDescription, RtcError> {
        let (mut native_wrapper, mut sdp_rx, mut err_rx) = create_sdp_observer();

        unsafe {
            self.sys_handle
                .create_offer(native_wrapper.pin_mut(), options.into());
        }

        futures::select! {
            sdp = sdp_rx => Ok(sdp.unwrap()),
            err = err_rx => Err(err.unwrap()),
        }
    }

    pub async fn create_answer(
        &self,
        options: AnswerOptions,
    ) -> Result<SessionDescription, RtcError> {
        let (mut native_wrapper, mut sdp_rx, mut err_rx) = create_sdp_observer();

        unsafe {
            self.sys_handle
                .create_answer(native_wrapper.pin_mut(), options.into());
        }

        futures::select! {
            sdp = sdp_rx => Ok(sdp.unwrap()),
            err = err_rx => Err(err.unwrap()),
        }
    }

    pub async fn set_local_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let (tx, rx) = oneshot::channel();
        let wrapper =
            sys_jsep::SetLocalSdpObserverWrapper(ManuallyDrop::new(Box::new(move |error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));

        let mut native_wrapper =
            sys_jsep::ffi::create_native_set_local_sdp_observer(Box::new(wrapper));

        unsafe {
            self.sys_handle
                .set_local_description(desc.handle.sys_handle, native_wrapper.pin_mut());
        }

        rx.await.unwrap().map_err(Into::into)
    }

    pub async fn set_remote_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let (tx, rx) = oneshot::channel();
        let wrapper =
            sys_jsep::SetRemoteSdpObserverWrapper(ManuallyDrop::new(Box::new(move |error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));

        let mut native_wrapper =
            sys_jsep::ffi::create_native_set_remote_sdp_observer(Box::new(wrapper));

        unsafe {
            self.sys_handle
                .set_remote_description(desc.handle.sys_handle, native_wrapper.pin_mut());
        }

        rx.await.unwrap().map_err(Into::into)
    }

    pub async fn add_ice_candidate(&self, candidate: IceCandidate) -> Result<(), RtcError> {
        let (tx, rx) = oneshot::channel();
        let observer =
            sys_pc::AddIceCandidateObserverWrapper(ManuallyDrop::new(Box::new(|error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));

        let mut native_observer =
            sys_pc::ffi::create_native_add_ice_candidate_observer(Box::new(observer));
        self.sys_handle
            .add_ice_candidate(candidate.handle.sys_handle, native_observer.pin_mut());

        rx.await.unwrap().map_err(Into::into)
    }

    pub fn create_data_channel(
        &self,
        label: &str,
        init: DataChannelInit,
    ) -> Result<DataChannel, RtcError> {
        let native_init = sys_dc::ffi::create_data_channel_init(init.into());
        let res = self
            .sys_handle
            .create_data_channel(label.to_string(), native_init);

        match res {
            Ok(sys_handle) => Ok(DataChannel {
                handle: imp_dc::DataChannel::configure(sys_handle),
            }),
            Err(e) => Err(unsafe { sys_err::ffi::RTCError::from(e.what()).into() }),
        }
    }

    pub fn add_track<T: AsRef<str>>(
        &self,
        track: MediaStreamTrack,
        stream_ids: &[T],
    ) -> Result<RtpSender, RtcError> {
        let stream_ids = stream_ids.iter().map(|s| s.as_ref().to_owned()).collect();
        let res = self.sys_handle.add_track(track.sys_handle(), &stream_ids);

        match res {
            Ok(sys_handle) => Ok(RtpSender {
                handle: imp_rs::RtpSender { sys_handle },
            }),
            Err(e) => unsafe { Err(sys_err::ffi::RTCError::from(e.what()).into()) },
        }
    }

    pub fn add_transceiver(
        &self,
        track: MediaStreamTrack,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        let res = self
            .sys_handle
            .add_transceiver(track.sys_handle(), init.into());

        match res {
            Ok(sys_handle) => Ok(RtpTransceiver {
                handle: imp_rt::RtpTransceiver {
                    sys_handle: sys_handle,
                },
            }),
            Err(e) => unsafe { Err(sys_err::ffi::RTCError::from(e.what()).into()) },
        }
    }

    pub fn add_transceiver_for_media(
        &self,
        media_type: MediaType,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        let res = self
            .sys_handle
            .add_transceiver_for_media(media_type.into(), init.into());

        match res {
            Ok(cxx_handle) => Ok(RtpTransceiver {
                handle: imp_rt::RtpTransceiver {
                    sys_handle: cxx_handle,
                },
            }),
            Err(e) => unsafe { Err(sys_err::ffi::RTCError::from(e.what()).into()) },
        }
    }

    pub fn close(&self) {
        self.sys_handle.close();
    }

    pub fn connection_state(&self) -> PeerConnectionState {
        self.sys_handle.connection_state().into()
    }

    pub fn ice_connection_state(&self) -> IceConnectionState {
        self.sys_handle.ice_connection_state().into()
    }

    pub fn ice_gathering_state(&self) -> IceGatheringState {
        self.sys_handle.ice_gathering_state().into()
    }

    pub fn signaling_state(&self) -> SignalingState {
        self.sys_handle.signaling_state().into()
    }

    pub fn current_local_description(&self) -> Option<SessionDescription> {
        let sdp = self.sys_handle.current_local_description();
        if sdp.is_null() {
            return None;
        }

        Some(SessionDescription {
            handle: imp_sdp::SessionDescription { sys_handle: sdp },
        })
    }

    pub fn current_remote_description(&self) -> Option<SessionDescription> {
        let sdp = self.sys_handle.current_remote_description();
        if sdp.is_null() {
            return None;
        }

        Some(SessionDescription {
            handle: imp_sdp::SessionDescription { sys_handle: sdp },
        })
    }

    pub fn remove_track(&self, sender: RtpSender) -> Result<(), RtcError> {
        self.sys_handle
            .remove_track(sender.handle.sys_handle)
            .map_err(|e| unsafe { sys_err::ffi::RTCError::from(e.what()).into() })
    }

    pub fn senders(&self) -> Vec<RtpSender> {
        self.sys_handle
            .get_senders()
            .into_iter()
            .map(|sender| RtpSender {
                handle: imp_rs::RtpSender {
                    sys_handle: sender.ptr,
                },
            })
            .collect()
    }

    pub fn receivers(&self) -> Vec<RtpReceiver> {
        self.sys_handle
            .get_receivers()
            .into_iter()
            .map(|receiver| RtpReceiver {
                handle: imp_rr::RtpReceiver {
                    sys_handle: receiver.ptr,
                },
            })
            .collect()
    }

    pub fn transceivers(&self) -> Vec<RtpTransceiver> {
        self.sys_handle
            .get_transceivers()
            .into_iter()
            .map(|transceiver| RtpTransceiver {
                handle: imp_rt::RtpTransceiver {
                    sys_handle: transceiver.ptr,
                },
            })
            .collect()
    }

    pub fn on_connection_state_change(&self, f: Option<OnConnectionChange>) {
        *self.observer.connection_change_handler.lock().unwrap() = f;
    }

    pub fn on_data_channel(&self, f: Option<OnDataChannel>) {
        *self.observer.data_channel_handler.lock().unwrap() = f;
    }

    pub fn on_ice_candidate(&self, f: Option<OnIceCandidate>) {
        *self.observer.ice_candidate_handler.lock().unwrap() = f;
    }

    pub fn on_ice_candidate_error(&self, f: Option<OnIceCandidateError>) {
        *self.observer.ice_candidate_error_handler.lock().unwrap() = f;
    }

    pub fn on_ice_connection_state_change(&self, f: Option<OnIceConnectionChange>) {
        *self.observer.ice_connection_change_handler.lock().unwrap() = f;
    }

    pub fn on_ice_gathering_state_change(&self, f: Option<OnIceGatheringChange>) {
        *self.observer.ice_gathering_change_handler.lock().unwrap() = f;
    }

    pub fn on_negotiation_needed(&self, f: Option<OnNegotiationNeeded>) {
        *self.observer.negotiation_needed_handler.lock().unwrap() = f;
    }

    pub fn on_signaling_state_change(&self, f: Option<OnSignalingChange>) {
        *self.observer.signaling_change_handler.lock().unwrap() = f;
    }

    pub fn on_track(&self, f: Option<OnTrack>) {
        *self.observer.track_handler.lock().unwrap() = f;
    }
}

fn create_sdp_observer() -> (
    UniquePtr<sys_pc::ffi::NativeCreateSdpObserverHandle>,
    oneshot::Receiver<SessionDescription>,
    oneshot::Receiver<RtcError>,
) {
    let (sdp_tx, sdp_rx) = oneshot::channel();
    let (err_tx, err_rx) = oneshot::channel();

    let wrapper = sys_jsep::CreateSdpObserverWrapper {
        on_success: ManuallyDrop::new(Box::new(move |session_description| {
            let _ = sdp_tx.send(SessionDescription {
                handle: imp_sdp::SessionDescription {
                    sys_handle: session_description,
                },
            });
        })),
        on_failure: ManuallyDrop::new(Box::new(move |error| {
            let _ = err_tx.send(error.into());
        })),
    };

    (
        sys_jsep::ffi::create_native_create_sdp_observer(Box::new(wrapper)),
        sdp_rx,
        err_rx,
    )
}

#[derive(Default)]
pub struct PeerObserver {
    pub connection_change_handler: Mutex<Option<OnConnectionChange>>,
    pub data_channel_handler: Mutex<Option<OnDataChannel>>,
    pub ice_candidate_handler: Mutex<Option<OnIceCandidate>>,
    pub ice_candidate_error_handler: Mutex<Option<OnIceCandidateError>>,
    pub ice_connection_change_handler: Mutex<Option<OnIceConnectionChange>>,
    pub ice_gathering_change_handler: Mutex<Option<OnIceGatheringChange>>,
    pub negotiation_needed_handler: Mutex<Option<OnNegotiationNeeded>>,
    pub signaling_change_handler: Mutex<Option<OnSignalingChange>>,
    pub track_handler: Mutex<Option<OnTrack>>,
}

impl sys_pc::PeerConnectionObserver for PeerObserver {
    fn on_signaling_change(&self, new_state: sys_pc::ffi::SignalingState) {
        if let Some(f) = self.signaling_change_handler.lock().unwrap().as_mut() {
            f(new_state.into());
        }
    }

    fn on_add_stream(&self, _stream: SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>) {}

    fn on_remove_stream(&self, _stream: SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>) {}

    fn on_data_channel(&self, data_channel: SharedPtr<sys_dc::ffi::DataChannel>) {
        if let Some(f) = self.data_channel_handler.lock().unwrap().as_mut() {
            f(DataChannel {
                handle: imp_dc::DataChannel::configure(data_channel),
            });
        }
    }

    fn on_renegotiation_needed(&self) {}

    fn on_negotiation_needed_event(&self, event: u32) {
        if let Some(f) = self.negotiation_needed_handler.lock().unwrap().as_mut() {
            f(event);
        }
    }

    fn on_ice_connection_change(&self, _new_state: sys_pc::ffi::IceConnectionState) {}

    fn on_standardized_ice_connection_change(&self, new_state: sys_pc::ffi::IceConnectionState) {
        if let Some(f) = self.ice_connection_change_handler.lock().unwrap().as_mut() {
            f(new_state.into());
        }
    }

    fn on_connection_change(&self, new_state: sys_pc::ffi::PeerConnectionState) {
        if let Some(f) = self.connection_change_handler.lock().unwrap().as_mut() {
            f(new_state.into());
        }
    }

    fn on_ice_gathering_change(&self, new_state: sys_pc::ffi::IceGatheringState) {
        if let Some(f) = self.ice_gathering_change_handler.lock().unwrap().as_mut() {
            f(new_state.into());
        }
    }

    fn on_ice_candidate(&self, candidate: SharedPtr<sys_jsep::ffi::IceCandidate>) {
        if let Some(f) = self.ice_candidate_handler.lock().unwrap().as_mut() {
            f(IceCandidate {
                handle: imp_ic::IceCandidate {
                    sys_handle: candidate,
                },
            });
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
        if let Some(f) = self.ice_candidate_error_handler.lock().unwrap().as_mut() {
            f(IceCandidateError {
                address,
                port,
                url,
                error_code,
                error_text,
            });
        }
    }

    fn on_ice_candidates_removed(
        &self,
        _removed: Vec<SharedPtr<webrtc_sys::candidate::ffi::Candidate>>,
    ) {
    }

    fn on_ice_connection_receiving_change(&self, _receiving: bool) {}

    fn on_ice_selected_candidate_pair_changed(
        &self,
        _event: sys_pc::ffi::CandidatePairChangeEvent,
    ) {
    }

    fn on_add_track(
        &self,
        _receiver: SharedPtr<webrtc_sys::rtp_receiver::ffi::RtpReceiver>,
        _streams: Vec<SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>>,
    ) {
    }

    fn on_track(&self, transceiver: SharedPtr<webrtc_sys::rtp_transceiver::ffi::RtpTransceiver>) {
        if let Some(f) = self.track_handler.lock().unwrap().as_mut() {
            let receiver = transceiver.receiver();
            let streams = receiver.streams();
            let track = receiver.track();

            f(TrackEvent {
                receiver: RtpReceiver {
                    handle: imp_rr::RtpReceiver {
                        sys_handle: receiver,
                    },
                },
                streams: streams
                    .into_iter()
                    .map(|s| MediaStream {
                        handle: imp_ms::MediaStream { sys_handle: s.ptr },
                    })
                    .collect(),
                track: imp_ms::new_media_stream_track(track),
                transceiver: RtpTransceiver {
                    handle: imp_rt::RtpTransceiver {
                        sys_handle: transceiver,
                    },
                },
            });
        }
    }

    fn on_remove_track(&self, _receiver: SharedPtr<webrtc_sys::rtp_receiver::ffi::RtpReceiver>) {}

    fn on_interesting_usage(&self, _usage_pattern: i32) {}
}
