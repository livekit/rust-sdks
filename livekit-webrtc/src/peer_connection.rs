use crate::prelude::*;
use cxx::{SharedPtr, UniquePtr};
use log::trace;
use std::fmt::{Debug, Formatter};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

use webrtc_sys::candidate as sys_ca;
use webrtc_sys::data_channel as sys_dc;
use webrtc_sys::jsep as sys_jsep;
use webrtc_sys::media_stream as sys_ms;
use webrtc_sys::peer_connection as sys_pc;
use webrtc_sys::rtp_receiver as sys_rr;
use webrtc_sys::rtp_sender as sys_rs;
use webrtc_sys::rtp_transceiver as sys_rt;

pub use webrtc_sys::peer_connection::ffi::IceConnectionState;
pub use webrtc_sys::peer_connection::ffi::IceGatheringState;
pub use webrtc_sys::peer_connection::ffi::PeerConnectionState;
pub use webrtc_sys::peer_connection::ffi::RTCOfferAnswerOptions;
pub use webrtc_sys::peer_connection::ffi::SignalingState;

pub struct PeerConnection {
    cxx_handle: UniquePtr<sys_pc::ffi::PeerConnection>,
    observer: Box<InternalObserver>,

    // Keep alive for C++
    #[allow(unused)]
    native_observer: UniquePtr<sys_pc::ffi::NativePeerConnectionObserver>,
}

impl Debug for PeerConnection {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("PeerConnection")
            .field("signaling_state", &self.signaling_state())
            .field("ice_connection_state", &self.ice_connection_state())
            .field("ice_gathering_state", &self.ice_gathering_state())
            .field("local_description", &self.local_description())
            .field("remote_description", &self.remote_description())
            .finish()
    }
}

impl PeerConnection {
    pub(crate) fn new(
        cxx_handle: UniquePtr<sys_pc::ffi::PeerConnection>,
        observer: Box<InternalObserver>,
        native_observer: UniquePtr<sys_pc::ffi::NativePeerConnectionObserver>,
    ) -> Self {
        Self {
            cxx_handle,
            observer,
            native_observer,
        }
    }

    fn create_sdp_observer() -> (
        UniquePtr<sys_pc::ffi::NativeCreateSdpObserverHandle>,
        mpsc::Receiver<Result<SessionDescription, RTCError>>,
    ) {
        let (tx, rx) = mpsc::channel(1);
        let wrapper = sys_jsep::CreateSdpObserverWrapper {
            on_success: ManuallyDrop::new(Box::new({
                let tx = tx.clone();
                move |session_description| {
                    let _ = tx.blocking_send(Ok(SessionDescription::new(session_description)));
                }
            })),
            on_failure: ManuallyDrop::new(Box::new(move |error| {
                let _ = tx.blocking_send(Err(error));
            })),
        };

        (
            sys_jsep::ffi::create_native_create_sdp_observer(Box::new(wrapper)),
            rx,
        )
    }

    pub async fn create_offer(
        &self,
        options: RTCOfferAnswerOptions,
    ) -> Result<SessionDescription, RTCError> {
        let (mut native_wrapper, mut rx) = Self::create_sdp_observer();

        unsafe {
            self.cxx_handle
                .create_offer(native_wrapper.pin_mut(), options);
        }

        rx.recv().await.unwrap()
    }

    pub async fn create_answer(
        &self,
        options: RTCOfferAnswerOptions,
    ) -> Result<SessionDescription, RTCError> {
        let (mut native_wrapper, mut rx) = Self::create_sdp_observer();

        unsafe {
            self.cxx_handle
                .create_answer(native_wrapper.pin_mut(), options);
        }

        rx.recv().await.unwrap()
    }

    pub async fn set_local_description(&self, desc: SessionDescription) -> Result<(), RTCError> {
        let (tx, rx) = oneshot::channel();
        let wrapper =
            sys_jsep::SetLocalSdpObserverWrapper(ManuallyDrop::new(Box::new(move |error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));
        let mut native_wrapper =
            sys_jsep::ffi::create_native_set_local_sdp_observer(Box::new(wrapper));

        unsafe {
            self.cxx_handle
                .set_local_description(desc.release(), native_wrapper.pin_mut());
        }

        rx.await.unwrap()
    }

    pub async fn set_remote_description(&self, desc: SessionDescription) -> Result<(), RTCError> {
        let (tx, rx) = oneshot::channel();
        let wrapper =
            sys_jsep::SetRemoteSdpObserverWrapper(ManuallyDrop::new(Box::new(move |error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));
        let mut native_wrapper =
            sys_jsep::ffi::create_native_set_remote_sdp_observer(Box::new(wrapper));

        unsafe {
            self.cxx_handle
                .set_remote_description(desc.release(), native_wrapper.pin_mut());
        }

        rx.await.unwrap()
    }

    pub fn create_data_channel(
        &self,
        label: &str,
        init: DataChannelInit,
    ) -> Result<DataChannel, RTCError> {
        let native_init = sys_dc::ffi::create_data_channel_init(init.into());
        let res = self
            .cxx_handle
            .create_data_channel(label.to_string(), native_init);

        match res {
            Ok(cxx_handle) => Ok(DataChannel::new(cxx_handle)),
            Err(e) => Err(unsafe { RTCError::from(e.what()) }),
        }
    }

    // TODO(theomonnom) Use IceCandidateInit instead of IceCandidate
    pub async fn add_ice_candidate(&self, candidate: IceCandidate) -> Result<(), RTCError> {
        let (tx, rx) = oneshot::channel();
        let observer =
            sys_pc::AddIceCandidateObserverWrapper(ManuallyDrop::new(Box::new(|error| {
                let _ = tx.send(if error.ok() { Ok(()) } else { Err(error) });
            })));

        let mut native_observer =
            sys_pc::ffi::create_native_add_ice_candidate_observer(Box::new(observer));
        self.cxx_handle
            .add_ice_candidate(candidate.release(), native_observer.pin_mut());

        rx.await.unwrap()
    }

    pub fn local_description(&self) -> Option<SessionDescription> {
        let local_description = self.cxx_handle.local_description();
        if local_description.is_null() {
            None
        } else {
            Some(SessionDescription::new(local_description))
        }
    }

    pub fn remote_description(&self) -> Option<SessionDescription> {
        let remote_description = self.cxx_handle.remote_description();
        if remote_description.is_null() {
            None
        } else {
            Some(SessionDescription::new(remote_description))
        }
    }

    pub fn signaling_state(&self) -> SignalingState {
        self.cxx_handle.signaling_state()
    }

    pub fn ice_gathering_state(&self) -> IceGatheringState {
        self.cxx_handle.ice_gathering_state()
    }

    pub fn ice_connection_state(&self) -> IceConnectionState {
        self.cxx_handle.ice_connection_state()
    }

    pub fn close(&mut self) {
        self.cxx_handle.pin_mut().close();
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
        *self
            .observer
            .on_renegotiation_needed_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_ice_connection_change(&mut self, handler: OnIceConnectionChangeHandler) {
        *self
            .observer
            .on_ice_connection_change_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_standardized_ice_connection_change(
        &mut self,
        handler: OnStandardizedIceConnectionChangeHandler,
    ) {
        *self
            .observer
            .on_standardized_ice_connection_change_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_connection_change(&mut self, handler: OnConnectionChangeHandler) {
        *self.observer.on_connection_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_gathering_change(&mut self, handler: OnIceGatheringChangeHandler) {
        *self
            .observer
            .on_ice_gathering_change_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_ice_candidate(&mut self, handler: OnIceCandidateHandler) {
        *self.observer.on_ice_candidate_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_candidate_error(&mut self, handler: OnIceCandidateErrorHandler) {
        *self.observer.on_ice_candidate_error_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_ice_candidates_removed(&mut self, handler: OnIceCandidatesRemovedHandler) {
        *self
            .observer
            .on_ice_candidates_removed_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_ice_connection_receiving_change(
        &mut self,
        handler: OnIceConnectionReceivingChangeHandler,
    ) {
        *self
            .observer
            .on_ice_connection_receiving_change_handler
            .lock()
            .unwrap() = Some(handler);
    }

    pub fn on_ice_selected_candidate_pair_changed(
        &mut self,
        handler: OnIceSelectedCandidatePairChangedHandler,
    ) {
        *self
            .observer
            .on_ice_selected_candidate_pair_changed_handler
            .lock()
            .unwrap() = Some(handler);
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

// TODO(theomonnom) Should we return futures?
pub type OnSignalingChangeHandler = Box<dyn FnMut(SignalingState) + Send + Sync>;
pub type OnAddStreamHandler = Box<dyn FnMut(MediaStream) + Send + Sync>;
pub type OnRemoveStreamHandler = Box<dyn FnMut(MediaStream) + Send + Sync>;
pub type OnDataChannelHandler = Box<dyn FnMut(DataChannel) + Send + Sync>;
pub type OnRenegotiationNeededHandler = Box<dyn FnMut() + Send + Sync>;
pub type OnNegotiationNeededEventHandler = Box<dyn FnMut(u32) + Send + Sync>;
pub type OnIceConnectionChangeHandler = Box<dyn FnMut(IceConnectionState) + Send + Sync>;
pub type OnStandardizedIceConnectionChangeHandler =
    Box<dyn FnMut(IceConnectionState) + Send + Sync>;
pub type OnConnectionChangeHandler = Box<dyn FnMut(PeerConnectionState) + Send + Sync>;
pub type OnIceGatheringChangeHandler = Box<dyn FnMut(IceGatheringState) + Send + Sync>;
pub type OnIceCandidateHandler = Box<dyn FnMut(IceCandidate) + Send + Sync>;
pub type OnIceCandidateErrorHandler =
    Box<dyn FnMut(String, i32, String, i32, String) + Send + Sync>;
pub type OnIceCandidatesRemovedHandler = Box<dyn FnMut(Vec<IceCandidate>) + Send + Sync>;
pub type OnIceConnectionReceivingChangeHandler = Box<dyn FnMut(bool) + Send + Sync>;
pub type OnIceSelectedCandidatePairChangedHandler =
    Box<dyn FnMut(webrtc_sys::peer_connection::ffi::CandidatePairChangeEvent) + Send + Sync>;
pub type OnAddTrackHandler = Box<dyn FnMut(RtpReceiver, Vec<MediaStream>) + Send + Sync>;
pub type OnTrackHandler = Box<dyn FnMut(RtpTransceiver) + Send + Sync>;
pub type OnRemoveTrackHandler = Box<dyn FnMut(RtpReceiver) + Send + Sync>;
pub type OnInterestingUsageHandler = Box<dyn FnMut(i32) + Send + Sync>;

pub(crate) struct InternalObserver {
    on_signaling_change_handler: Arc<Mutex<Option<OnSignalingChangeHandler>>>,
    on_add_stream_handler: Arc<Mutex<Option<OnAddStreamHandler>>>,
    on_remove_stream_handler: Arc<Mutex<Option<OnRemoveStreamHandler>>>,
    on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHandler>>>,
    on_renegotiation_needed_handler: Arc<Mutex<Option<OnRenegotiationNeededHandler>>>,
    on_negotiation_needed_event_handler: Arc<Mutex<Option<OnNegotiationNeededEventHandler>>>,
    on_ice_connection_change_handler: Arc<Mutex<Option<OnIceConnectionChangeHandler>>>,
    on_standardized_ice_connection_change_handler:
        Arc<Mutex<Option<OnStandardizedIceConnectionChangeHandler>>>,
    on_connection_change_handler: Arc<Mutex<Option<OnConnectionChangeHandler>>>,
    on_ice_gathering_change_handler: Arc<Mutex<Option<OnIceGatheringChangeHandler>>>,
    on_ice_candidate_handler: Arc<Mutex<Option<OnIceCandidateHandler>>>,
    on_ice_candidate_error_handler: Arc<Mutex<Option<OnIceCandidateErrorHandler>>>,
    on_ice_candidates_removed_handler: Arc<Mutex<Option<OnIceCandidatesRemovedHandler>>>,
    on_ice_connection_receiving_change_handler:
        Arc<Mutex<Option<OnIceConnectionReceivingChangeHandler>>>,
    on_ice_selected_candidate_pair_changed_handler:
        Arc<Mutex<Option<OnIceSelectedCandidatePairChangedHandler>>>,
    on_add_track_handler: Arc<Mutex<Option<OnAddTrackHandler>>>,
    on_track_handler: Arc<Mutex<Option<OnTrackHandler>>>,
    on_remove_track_handler: Arc<Mutex<Option<OnRemoveTrackHandler>>>,
    on_interesting_usage_handler: Arc<Mutex<Option<OnInterestingUsageHandler>>>,
}

impl Default for InternalObserver {
    fn default() -> Self {
        Self {
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
            on_interesting_usage_handler: Arc::new(Default::default()),
        }
    }
}

// Observers are being called on the Signaling Thread
impl sys_pc::PeerConnectionObserver for InternalObserver {
    fn on_signaling_change(&self, new_state: SignalingState) {
        trace!("on_signaling_change, {:?}", new_state);
        let mut handler = self.on_signaling_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_add_stream(&self, stream: SharedPtr<sys_ms::ffi::MediaStream>) {
        trace!("on_add_stream");
        let mut handler = self.on_add_stream_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_remove_stream(&self, stream: SharedPtr<sys_ms::ffi::MediaStream>) {
        trace!("on_remove_stream");
        let mut handler = self.on_remove_stream_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_data_channel(&self, data_channel: UniquePtr<sys_dc::ffi::DataChannel>) {
        trace!("on_data_channel");
        let mut handler = self.on_data_channel_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(DataChannel::new(data_channel));
        }
    }

    fn on_renegotiation_needed(&self) {
        trace!("on_renegotiation_needed");
        let mut handler = self.on_renegotiation_needed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f();
        }
    }

    fn on_negotiation_needed_event(&self, event: u32) {
        trace!("on_negotiation_needed_event");
        let mut handler = self.on_negotiation_needed_event_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(event);
        }
    }

    fn on_ice_connection_change(&self, new_state: IceConnectionState) {
        trace!("on_ice_connection_change (new_state: {:?})", new_state);
        let mut handler = self.on_ice_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_standardized_ice_connection_change(&self, new_state: IceConnectionState) {
        trace!(
            "on_standardized_ice_connection_change (new_state: {:?}",
            new_state
        );
        let mut handler = self
            .on_standardized_ice_connection_change_handler
            .lock()
            .unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_connection_change(&self, new_state: PeerConnectionState) {
        trace!("on_connection_change (new_state: {:?})", new_state);
        let mut handler = self.on_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_ice_gathering_change(&self, new_state: IceGatheringState) {
        trace!("on_ice_gathering_change (new_state: {:?}", new_state);
        let mut handler = self.on_ice_gathering_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(new_state);
        }
    }

    fn on_ice_candidate(&self, candidate: SharedPtr<sys_jsep::ffi::IceCandidate>) {
        trace!("on_ice_candidate");
        let mut handler = self.on_ice_candidate_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(IceCandidate::new(candidate));
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
        trace!("on_ice_candidate_error");
        let mut handler = self.on_ice_candidate_error_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(address, port, url, error_code, error_text);
        }
    }

    fn on_ice_candidates_removed(&self, removed: Vec<SharedPtr<sys_ca::ffi::Candidate>>) {
        trace!("on_ice_candidates_removed");
        let mut handler = self.on_ice_candidates_removed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_ice_connection_receiving_change(&self, receiving: bool) {
        trace!("on_ice_connection_receiving_change");
        let mut handler = self
            .on_ice_connection_receiving_change_handler
            .lock()
            .unwrap();
        if let Some(f) = handler.as_mut() {
            f(receiving);
        }
    }

    fn on_ice_selected_candidate_pair_changed(&self, event: sys_pc::ffi::CandidatePairChangeEvent) {
        trace!("on_ice_selected_candidate_pair_changed");
        let mut handler = self
            .on_ice_selected_candidate_pair_changed_handler
            .lock()
            .unwrap();
        if let Some(f) = handler.as_mut() {
            f(event);
        }
    }

    fn on_add_track(
        &self,
        receiver: SharedPtr<sys_rr::ffi::RtpReceiver>,
        streams: Vec<SharedPtr<sys_ms::ffi::MediaStream>>,
    ) {
        trace!("on_add_track");
        let mut handler = self.on_add_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            let streams = streams.into_iter().map(MediaStream::new).collect();
            f(RtpReceiver::new(receiver), streams)
        }
    }

    fn on_track(&self, transceiver: SharedPtr<sys_rt::ffi::RtpTransceiver>) {
        trace!("on_track");
        let mut handler = self.on_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_remove_track(&self, receiver: SharedPtr<sys_rr::ffi::RtpReceiver>) {
        trace!("on_remove_track");
        let mut handler = self.on_remove_track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            // TODO(theomonnom)
        }
    }

    fn on_interesting_usage(&self, usage_pattern: i32) {
        trace!("on_interesting_usage");
        let mut handler = self.on_interesting_usage_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(usage_pattern);
        }
    }
}

#[cfg(test)]
mod tests {
    use log::trace;
    use tokio::sync::mpsc;

    use webrtc_sys::peer_connection::ffi::RTCOfferAnswerOptions;
    use webrtc_sys::peer_connection_factory::ffi::{ContinualGatheringPolicy, IceTransportsType};

    use crate::data_channel::{DataChannel, DataChannelInit};
    use crate::jsep::IceCandidate;
    use crate::peer_connection_factory::{ICEServer, PeerConnectionFactory, RTCConfiguration};
    use crate::webrtc::RTCRuntime;

    fn init_log() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[tokio::test]
    async fn create_pc() {
        init_log();

        let rtc_runtime = RTCRuntime::new();

        let factory = PeerConnectionFactory::new(rtc_runtime);
        let config = RTCConfiguration {
            ice_servers: vec![ICEServer {
                urls: vec!["stun:stun1.l.google.com:19302".to_string()],
                username: "".into(),
                password: "".into(),
            }],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        let mut bob = factory.create_peer_connection(config.clone()).unwrap();
        let mut alice = factory.create_peer_connection(config.clone()).unwrap();

        let (bob_ice_tx, mut bob_ice_rx) = mpsc::channel::<IceCandidate>(16);
        let (alice_ice_tx, mut alice_ice_rx) = mpsc::channel::<IceCandidate>(16);
        let (alice_dc_tx, mut alice_dc_rx) = mpsc::channel::<DataChannel>(16);

        bob.on_ice_candidate(Box::new(move |candidate| {
            bob_ice_tx.blocking_send(candidate).unwrap();
        }));

        alice.on_ice_candidate(Box::new(move |candidate| {
            alice_ice_tx.blocking_send(candidate).unwrap();
        }));

        alice.on_data_channel(Box::new(move |dc| {
            alice_dc_tx.blocking_send(dc).unwrap();
        }));

        let mut bob_dc = bob
            .create_data_channel("test_dc", DataChannelInit::default())
            .unwrap();

        let offer = bob
            .create_offer(RTCOfferAnswerOptions::default())
            .await
            .unwrap();
        trace!("Bob offer: {:?}", offer);
        bob.set_local_description(offer.clone()).await.unwrap();
        alice.set_remote_description(offer).await.unwrap();

        let answer = alice
            .create_answer(RTCOfferAnswerOptions::default())
            .await
            .unwrap();

        trace!("Alice answer: {:?}", answer);
        alice.set_local_description(answer.clone()).await.unwrap();
        bob.set_remote_description(answer).await.unwrap();

        let bob_ice = bob_ice_rx.recv().await.unwrap();
        let alice_ice = alice_ice_rx.recv().await.unwrap();

        bob.add_ice_candidate(alice_ice).await.unwrap();
        alice.add_ice_candidate(bob_ice).await.unwrap();

        let (data_tx, mut data_rx) = mpsc::channel::<String>(1);
        let mut alice_dc = alice_dc_rx.recv().await.unwrap();
        alice_dc.on_message(Box::new(move |data, _| {
            data_tx
                .blocking_send(String::from_utf8_lossy(data).to_string())
                .unwrap();
        }));

        bob_dc.send(b"This is a test", true).unwrap();
        assert_eq!(data_rx.recv().await.unwrap(), "This is a test");

        alice.close();
        bob.close();
    }
}
