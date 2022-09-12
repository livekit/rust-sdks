use std::sync::{Arc, Mutex};
use cxx::UniquePtr;
use tokio::sync::mpsc;
use libwebrtc_sys::peer_connection as sys_pc;
use libwebrtc_sys::jsep as sys_jsep;

use crate::data_channel::DataChannel;
use crate::media_stream::MediaStream;
use crate::ice_candidate::IceCandidate;
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_transceiver::RtpTransceiver;
use crate::session_description::SessionDescription;
use crate::rtc_error::RTCError;

pub use libwebrtc_sys::peer_connection::ffi::PeerConnectionState;
pub use libwebrtc_sys::peer_connection::ffi::SignalingState;
pub use libwebrtc_sys::peer_connection::ffi::IceConnectionState;
pub use libwebrtc_sys::peer_connection::ffi::IceGatheringState;
pub use libwebrtc_sys::peer_connection::ffi::RTCOfferAnswerOptions;

pub struct PeerConnection {
    cxx_handle: UniquePtr<sys_pc::ffi::PeerConnection>,
    observer: InternalObserver
}

impl PeerConnection {
    pub fn new(cxx_handle: UniquePtr<sys_pc::ffi::PeerConnection>) -> Self {
        Self {
            cxx_handle,
            observer: InternalObserver {
                on_signaling_change_handler: Arc::new(Default::default()),
                on_add_stream_handler: Arc::new(Default::default()),
                on_remove_stream_handler: Arc::new(Default::default()),
                on_data_channel_handler: Arc::new(Default::default()),
                on_renegotiation_needed_handler: Arc::new(Default::default()),
                on_negotiation_needed_event_handler: Arc::new(Default::default()),
                on_ice_connection_change_handler: Arc::new(Default::default()),
                on_standardized_ice_connection_change_handler: Arc::new(Default::default()),
                on_connection_change_handler: Arc::new(Default::default()),
                on_ice_gathering_change_handler: Arc::new(Default::default()),
                on_ice_candidate_handler: Arc::new(Default::default()),
                on_ice_candidate_error_handler: Arc::new(Default::default()),
                on_ice_candidates_removed_handler: Arc::new(Default::default()),
                on_ice_connection_receiving_change_handler: Arc::new(Default::default()),
                on_ice_selected_candidate_pair_changed_handler: Arc::new(Default::default()),
                on_add_track_handler: Arc::new(Default::default()),
                on_track_handler: Arc::new(Default::default()),
                on_remove_track_handler: Arc::new(Default::default()),
                on_interesting_usage_handler: Arc::new(Default::default())
            }
        }
    }

    pub async fn create_offer(&mut self) -> Result<SessionDescription, RTCError> {
        let (tx, mut rx) = mpsc::channel(1);

        let wrapper = sys_jsep::CreateSdpObserverWrapper::new(Box::new(InternalCreateSdpObserver { tx }));
        let native_wrapper = sys_jsep::ffi::create_native_create_sdp_observer(Box::new(wrapper));

        self.cxx_handle.pin_mut().create_offer(native_wrapper, RTCOfferAnswerOptions::default());
        rx.recv().await.unwrap()
    }

    pub async fn create_answer(&mut self) -> Result<SessionDescription, RTCError> {

    }

    pub fn on_signaling_change(&mut self, handler: OnSignalingChangeHandler) {
        *self.observer.on_signaling_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_add_stream(&mut self, handler: OnAddStreamHandler) {
        *self.observer.on_add_stream_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_remove_stream(&mut self, handler: OnRemoveStreamHandler) {
        *self.observer.on_remove_stream_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_data_channel(&mut self, handler: OnDataChannelHandler) {
        *self.observer.on_data_channel_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_renegotiation_needed(&mut self, handler: OnRenegotiationNeededHandler) {
        *self.observer.on_renegotiation_needed_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_connection_change(&mut self, handler: OnIceConnectionChangeHandler) {
        *self.observer.on_ice_connection_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_standardized_ice_connection_change(&mut self, handler: OnStandardizedIceConnectionChangeHandler) {
        *self.observer.on_standardized_ice_connection_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_connection_change(&mut self, handler: OnConnectionChangeHandler) {
        *self.observer.on_connection_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_gathering_change(&mut self, handler: OnIceGatheringChangeHandler) {
        *self.observer.on_ice_gathering_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_candidate(&mut self, handler: OnIceCandidateHandler) {
        *self.observer.on_ice_candidate_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_candidate_error(&mut self, handler: OnIceCandidateErrorHandler) {
        *self.observer.on_ice_candidate_error_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_candidates_removed(&mut self, handler: OnIceCandidatesRemovedHandler) {
        *self.observer.on_ice_candidates_removed_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_connection_receiving_change(&mut self, handler: OnIceConnectionReceivingChangeHandler) {
        *self.observer.on_ice_connection_receiving_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_selected_candidate_pair_changed(&mut self, handler: OnIceSelectedCandidatePairChangedHandler) {
        *self.observer.on_ice_selected_candidate_pair_changed_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_add_track(&mut self, handler: OnAddTrackHandler) {
        *self.observer.on_add_track_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_track(&mut self, handler: OnTrackHandler) {
        *self.observer.on_track_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_remove_track(&mut self, handler: OnRemoveTrackHandler) {
        *self.observer.on_remove_track_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_interesting_usage(&mut self, handler: OnInterestingUsageHandler) {
        *self.observer.on_interesting_usage_handler.lock().unwrap() = Some(handler);
    }
}

// CreateSdpObserver

struct InternalCreateSdpObserver {
    tx: mpsc::Sender<Result<SessionDescription, RTCError>>
}

impl sys_jsep::CreateSdpObserver for InternalCreateSdpObserver {
    fn on_success(&self, session_description: UniquePtr<libwebrtc_sys::jsep::ffi::SessionDescription>) {
        self.tx.blocking_send(Ok(SessionDescription{})).unwrap(); // TODO
    }

    fn on_failure(&self, error: UniquePtr<libwebrtc_sys::rtc_error::ffi::RTCError>) {
        self.tx.blocking_send(Err(RTCError{})).unwrap(); // TODO
    }
}

// PeerConnectionObserver

// TODO(theomonnom) Should we return futures?
pub type OnSignalingChangeHandler = Box<dyn FnMut(SignalingState) + Send + Sync>;
pub type OnAddStreamHandler = Box<dyn FnMut(MediaStream) + Send + Sync>;
pub type OnRemoveStreamHandler = Box<dyn FnMut(MediaStream) + Send + Sync>;
pub type OnDataChannelHandler = Box<dyn FnMut(DataChannel) + Send + Sync>;
pub type OnRenegotiationNeededHandler = Box<dyn FnMut() + Send + Sync>;
pub type OnNegotiationNeededEventHandler = Box<dyn FnMut(u32) + Send + Sync>;
pub type OnIceConnectionChangeHandler = Box<dyn FnMut(IceConnectionState) + Send + Sync>;
pub type OnStandardizedIceConnectionChangeHandler = Box<dyn FnMut(IceConnectionState) + Send + Sync>;
pub type OnConnectionChangeHandler = Box<dyn FnMut(PeerConnectionState) + Send + Sync>;
pub type OnIceGatheringChangeHandler = Box<dyn FnMut(IceGatheringState) + Send + Sync>;
pub type OnIceCandidateHandler = Box<dyn FnMut(IceCandidate) + Send + Sync>;
pub type OnIceCandidateErrorHandler = Box<dyn FnMut(String, i32, String, i32, String) + Send + Sync>;
pub type OnIceCandidatesRemovedHandler = Box<dyn FnMut(Vec<IceCandidate>) + Send + Sync>;
pub type OnIceConnectionReceivingChangeHandler = Box<dyn FnMut(bool) + Send + Sync>;
pub type OnIceSelectedCandidatePairChangedHandler = Box<dyn FnMut(libwebrtc_sys::peer_connection::ffi::CandidatePairChangeEvent) + Send + Sync>;
pub type OnAddTrackHandler = Box<dyn FnMut(RtpReceiver, Vec<MediaStream>) + Send + Sync>;
pub type OnTrackHandler = Box<dyn FnMut(RtpTransceiver) + Send + Sync>;
pub type OnRemoveTrackHandler = Box<dyn FnMut(RtpReceiver) + Send + Sync>;
pub type OnInterestingUsageHandler = Box<dyn FnMut(i32) + Send + Sync>;

struct InternalObserver {
    on_signaling_change_handler: Arc<Mutex<Option<OnSignalingChangeHandler>>>,
    on_add_stream_handler: Arc<Mutex<Option<OnAddStreamHandler>>>,
    on_remove_stream_handler: Arc<Mutex<Option<OnRemoveStreamHandler>>>,
    on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHandler>>>,
    on_renegotiation_needed_handler: Arc<Mutex<Option<OnRenegotiationNeededHandler>>>,
    on_negotiation_needed_event_handler: Arc<Mutex<Option<OnNegotiationNeededEventHandler>>>,
    on_ice_connection_change_handler: Arc<Mutex<Option<OnIceConnectionChangeHandler>>>,
    on_standardized_ice_connection_change_handler: Arc<Mutex<Option<OnStandardizedIceConnectionChangeHandler>>>,
    on_connection_change_handler: Arc<Mutex<Option<OnConnectionChangeHandler>>>,
    on_ice_gathering_change_handler: Arc<Mutex<Option<OnIceGatheringChangeHandler>>>,
    on_ice_candidate_handler: Arc<Mutex<Option<OnIceCandidateHandler>>>,
    on_ice_candidate_error_handler: Arc<Mutex<Option<OnIceCandidateErrorHandler>>>,
    on_ice_candidates_removed_handler: Arc<Mutex<Option<OnIceCandidatesRemovedHandler>>>,
    on_ice_connection_receiving_change_handler: Arc<Mutex<Option<OnIceConnectionReceivingChangeHandler>>>,
    on_ice_selected_candidate_pair_changed_handler: Arc<Mutex<Option<OnIceSelectedCandidatePairChangedHandler>>>,
    on_add_track_handler: Arc<Mutex<Option<OnAddTrackHandler>>>,
    on_track_handler: Arc<Mutex<Option<OnTrackHandler>>>,
    on_remove_track_handler: Arc<Mutex<Option<OnRemoveTrackHandler>>>,
    on_interesting_usage_handler: Arc<Mutex<Option<OnInterestingUsageHandler>>>
}

// Observers are being called on the Signaling Thread
impl sys_pc::PeerConnectionObserver for InternalObserver {
    fn on_signaling_change(&mut self, new_state: SignalingState) {
        let mut handler = self.on_signaling_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_add_stream(&mut self, stream: UniquePtr<libwebrtc_sys::media_stream_interface::ffi::MediaStreamInterface>) {
        let mut handler = self.on_add_stream_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_remove_stream(&mut self, stream: UniquePtr<libwebrtc_sys::media_stream_interface::ffi::MediaStreamInterface>) {
        let mut handler = self.on_remove_stream_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_data_channel(&mut self, data_channel: UniquePtr<libwebrtc_sys::data_channel::ffi::DataChannel>) {
        let mut handler = self.on_data_channel_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_renegotiation_needed(&mut self) {
        let mut handler = self.on_renegotiation_needed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f();
        }
    }

    fn on_negotiation_needed_event(&mut self, event: u32) {
        let mut handler = self.on_negotiation_needed_event_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(event);
        }
    }

    fn on_ice_connection_change(&mut self, new_state: IceConnectionState) {
        let mut handler = self.on_ice_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_standardized_ice_connection_change(&mut self, new_state: IceConnectionState) {
        let mut handler = self.on_standardized_ice_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_connection_change(&mut self, new_state: PeerConnectionState) {
        let mut handler = self.on_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_ice_gathering_change(&mut self, new_state: IceGatheringState) {
        let mut handler = self.on_ice_gathering_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_ice_candidate(&mut self, candidate: UniquePtr<libwebrtc_sys::jsep::ffi::IceCandidate>) {
        let mut handler = self.on_ice_candidate_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_ice_candidate_error(&mut self, address: String, port: i32, url: String, error_code: i32, error_text: String) {
        let mut handler = self.on_ice_candidate_error_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(address, port, url, error_code, error_text);
        }
    }

    fn on_ice_candidates_removed(&mut self, removed: Vec<UniquePtr<libwebrtc_sys::candidate::ffi::Candidate>>) {
        let mut handler = self.on_ice_candidates_removed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_ice_connection_receiving_change(&mut self, receiving: bool) {
        let mut handler = self.on_ice_connection_receiving_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(receiving);
        }
    }

    fn on_ice_selected_candidate_pair_changed(&mut self, event: libwebrtc_sys::peer_connection::ffi::CandidatePairChangeEvent) {
        let mut handler = self.on_ice_selected_candidate_pair_changed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(event);
        }
    }

    fn on_add_track(&mut self, receiver: UniquePtr<libwebrtc_sys::rtp_receiver::ffi::RtpReceiver>, streams: Vec<UniquePtr<libwebrtc_sys::media_stream_interface::ffi::MediaStreamInterface>>) {
        let mut handler = self.on_add_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_track(&mut self, transceiver: UniquePtr<libwebrtc_sys::rtp_transceiver::ffi::RtpTransceiver>) {
        let mut handler = self.on_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_remove_track(&mut self, receiver: UniquePtr<libwebrtc_sys::rtp_receiver::ffi::RtpReceiver>) {
        let mut handler = self.on_remove_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_interesting_usage(&mut self, usage_pattern: i32) {
        let mut handler = self.on_interesting_usage_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(usage_pattern);
        }
    }
}