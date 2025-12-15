// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;
use std::{fmt::Debug, sync::Mutex};
use tokio::sync::mpsc;

use crate::data_channel::{DataChannel, DataChannelInit};
use crate::ice_candidate::IceCandidate;
use crate::media_stream::MediaStream;
use crate::media_stream_track::{new_media_stream_track, MediaStreamTrack};
use crate::rtp_parameters::RtpTransceiverInit;
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_sender::RtpSender;
use crate::rtp_transceiver::RtpTransceiver;
use crate::session_description::SessionDescription;
use crate::stats::RtcStats;
use crate::sys::{self, *};
use crate::{MediaType, RtcError, RtcErrorType};

use crate::peer_connection_factory::RtcConfiguration;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PeerConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceConnectionState {
    New,
    Checking,
    Connected,
    Completed,
    Failed,
    Disconnected,
    Closed,
    Max,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceGatheringState {
    New,
    Gathering,
    Complete,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveLocalPrAnswer,
    HaveRemoteOffer,
    HaveRemotePrAnswer,
    Closed,
}

#[derive(Debug, Clone)]
pub struct OfferOptions {
    pub ice_restart: bool,
    pub use_rtp_mux: bool,
    pub offer_to_receive_audio: bool,
    pub offer_to_receive_video: bool,
}

impl Default for OfferOptions {
    fn default() -> Self {
        Self {
            ice_restart: false,
            use_rtp_mux: true,
            offer_to_receive_audio: false,
            offer_to_receive_video: false,
        }
    }
}

impl From<OfferOptions> for lkOfferAnswerOptions {
    fn from(_options: OfferOptions) -> Self {
        lkOfferAnswerOptions {
            iceRestart: _options.ice_restart,
            useRtpMux: _options.use_rtp_mux,
            offerToReceiveAudio: _options.offer_to_receive_audio,
            offerToReceiveVideo: _options.offer_to_receive_video,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnswerOptions {}

impl From<AnswerOptions> for lkOfferAnswerOptions {
    fn from(_options: AnswerOptions) -> Self {
        lkOfferAnswerOptions {
            iceRestart: false,
            useRtpMux: true,
            offerToReceiveAudio: false,
            offerToReceiveVideo: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IceCandidateError {
    pub address: String,
    pub port: i32,
    pub url: String,
    pub error_code: i32,
    pub error_text: String,
}
#[derive(Debug, Clone)]
pub struct TrackEvent {
    pub receiver: RtpReceiver,
    pub streams: Vec<MediaStream>,
    pub track: MediaStreamTrack,
    pub transceiver: RtpTransceiver,
}

pub type OnConnectionChange = Box<dyn FnMut(PeerConnectionState) + Send + Sync>;
pub type OnDataChannel = Box<dyn FnMut(DataChannel) + Send + Sync>;
pub type OnIceCandidate = Box<dyn FnMut(IceCandidate) + Send + Sync>;
pub type OnIceCandidateError = Box<dyn FnMut(IceCandidateError) + Send + Sync>;
pub type OnIceConnectionChange = Box<dyn FnMut(IceConnectionState) + Send + Sync>;
pub type OnIceGatheringChange = Box<dyn FnMut(IceGatheringState) + Send + Sync>;
pub type OnNegotiationNeeded = Box<dyn FnMut(u32) + Send + Sync>;
pub type OnSignalingChange = Box<dyn FnMut(SignalingState) + Send + Sync>;
pub type OnTrack = Box<dyn FnMut(TrackEvent) + Send + Sync>;

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

impl From<lkSignalingState> for SignalingState {
    fn from(value: lkSignalingState) -> Self {
        match value {
            lkSignalingState::LK_SIGNALING_STATE_STABLE => Self::Stable,
            lkSignalingState::LK_SIGNALING_STATE_HAVE_LOCAL_OFFER => Self::HaveLocalOffer,
            lkSignalingState::LK_SIGNALING_STATE_HAVE_REMOTE_OFFER => Self::HaveRemoteOffer,
            lkSignalingState::LK_SIGNALING_STATE_HAVE_LOCAL_PRANSWER => Self::HaveLocalPrAnswer,
            lkSignalingState::LK_SIGNALING_STATE_HAVE_REMOTE_PRANSWER => Self::HaveRemotePrAnswer,
            lkSignalingState::LK_SIGNALING_STATE_CLOSED => Self::Closed,
        }
    }
}

impl From<lkIceState> for IceConnectionState {
    fn from(value: lkIceState) -> Self {
        match value {
            lkIceState::LK_ICE_STATE_NEW => Self::New,
            lkIceState::LK_ICE_STATE_CHECKING => Self::Checking,
            lkIceState::LK_ICE_STATE_CLOSED => Self::Closed,
            lkIceState::LK_ICE_STATE_COMPLETED => Self::Completed,
            lkIceState::LK_ICE_STATE_CONNECTED => Self::Connected,
            lkIceState::LK_ICE_STATE_DISCONNECTED => Self::Disconnected,
            lkIceState::LK_ICE_STATE_FAILED => Self::Failed,
        }
    }
}

impl From<lkPeerState> for PeerConnectionState {
    fn from(value: lkPeerState) -> Self {
        match value {
            lkPeerState::LK_PEER_STATE_NEW => Self::New,
            lkPeerState::LK_PEER_STATE_CLOSED => Self::Closed,
            lkPeerState::LK_PEER_STATE_CONNECTED => Self::Connected,
            lkPeerState::LK_PEER_STATE_CONNECTING => Self::Connecting,
            lkPeerState::LK_PEER_STATE_DISCONNECTED => Self::Disconnected,
            lkPeerState::LK_PEER_STATE_FAILED => Self::Failed,
        }
    }
}

impl From<lkIceGatheringState> for IceGatheringState {
    fn from(value: lkIceGatheringState) -> Self {
        match value {
            lkIceGatheringState::LK_ICE_GATHERING_NEW => Self::New,
            lkIceGatheringState::LK_ICE_GATHERING_GATHERING => Self::Gathering,
            lkIceGatheringState::LK_ICE_GATHERING_COMPLETE => Self::Complete,
        }
    }
}

impl PeerObserver {
    pub extern "C" fn peer_on_signaling_change(
        state: lkSignalingState,
        userdata: *mut std::ffi::c_void,
    ) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.signaling_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    pub extern "C" fn peer_on_ice_candidate(
        candidate: *mut lkIceCandidate,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.ice_candidate_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(IceCandidate { ffi: unsafe { sys::RefCounted::from_raw(candidate) } });
        }
    }

    pub extern "C" fn peer_on_data_channel(
        lk_dc: *const lkDataChannel,
        userdata: *mut std::ffi::c_void,
    ) {
        println!("peer_on_data_channel called with dc: {:?}", lk_dc);
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.data_channel_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            let dc = DataChannel::configure(unsafe { sys::RefCounted::from_raw(lk_dc as *mut _) });
            f(dc);
        }
    }

    pub extern "C" fn peer_on_track(
        transceiver: *const lkRtpTransceiver,
        receiver: *const lkRtpReceiver,
        streams: *const lkVectorGeneric,
        track: *const lkMediaStreamTrack,
        userdata: *mut std::ffi::c_void,
    ) {
        let lk_transceiver =
            RtpTransceiver { ffi: unsafe { sys::RefCounted::from_raw(transceiver as *mut _) } };
        let lk_receiver =
            RtpReceiver { ffi: unsafe { sys::RefCounted::from_raw(receiver as *mut _) } };
        let lk_track =
            new_media_stream_track(unsafe { sys::RefCounted::from_raw(track as *mut _) });
        let mut lk_streams = Vec::new();
        let stream_vec = sys::RefCountedVector::from_native_vec(streams as *mut _);
        for i in 0..stream_vec.vec.len() as isize {
            lk_streams.push(MediaStream { ffi: stream_vec.vec[i as usize].clone() });
        }

        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(TrackEvent {
                transceiver: lk_transceiver,
                receiver: lk_receiver,
                streams: lk_streams,
                track: lk_track,
            });
        }
    }

    pub extern "C" fn peer_on_remove_track(
        receiver: *const lkRtpReceiver,
        userdata: *mut std::ffi::c_void,
    ) {
        let _lk_receiver =
            RtpReceiver { ffi: unsafe { sys::RefCounted::from_raw(receiver as *mut _) } };
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let _binding = observer.lock().unwrap();
        // Currently no handler for remove track
    }

    pub extern "C" fn peer_on_connection_state_change(
        state: lkPeerState,
        userdata: *mut std::ffi::c_void,
    ) {
        println!("peer_on_connection_state_change called with state: {:?}", state);
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    pub extern "C" fn peer_on_renegotiation_needed(userdata: *mut std::os::raw::c_void) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.negotiation_needed_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(0);
        }
    }

    pub extern "C" fn peer_on_ice_gathering_change(
        state: lkIceGatheringState,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.ice_gathering_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    pub extern "C" fn peer_on_standardized_ice_connection_change(
        state: lkIceState,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.ice_connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    pub extern "C" fn peer_on_ice_candidate_error(
        address: *const ::std::os::raw::c_char,
        port: ::std::os::raw::c_int,
        url: *const ::std::os::raw::c_char,
        error_code: ::std::os::raw::c_int,
        error_text: *const ::std::os::raw::c_char,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer: &mut Mutex<PeerObserver> =
            unsafe { &mut *userdata.cast::<Mutex<PeerObserver>>() };
        let binding = observer.lock().unwrap();
        let mut handler = binding.ice_candidate_error_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(IceCandidateError {
                address: unsafe { std::ffi::CStr::from_ptr(address).to_str().unwrap().to_string() },
                port,
                url: unsafe { std::ffi::CStr::from_ptr(url).to_str().unwrap().to_string() },
                error_code,
                error_text: unsafe {
                    std::ffi::CStr::from_ptr(error_text).to_str().unwrap().to_string()
                },
            });
        }
    }
}

#[derive(Clone)]
pub struct PeerConnection {
    pub(crate) observer: Arc<Mutex<PeerObserver>>,
    pub(crate) ffi: sys::RefCounted<sys::lkPeer>,
}

impl PeerConnection {
    pub fn set_configuration(&self, config: RtcConfiguration) -> Result<(), RtcError> {
        let sys_config: sys::lkRtcConfiguration = config.into();
        unsafe {
            sys::lkPeerSetConfig(self.ffi.as_ptr(), &sys_config);
        }
        Ok(())
    }

    pub async fn create_offer(
        &self,
        options: OfferOptions,
    ) -> Result<SessionDescription, RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<SessionDescription, RtcError>>(1);
        let tx_box = Box::new(tx);
        type CtxType = mpsc::Sender<Result<SessionDescription, RtcError>>;

        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        // Prepare observer callbacks
        unsafe extern "C" fn create_offer_on_success(
            desc: *mut lkSessionDescription,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Ok(SessionDescription {
                ffi: unsafe { sys::RefCounted::from_raw(desc) },
            }));
        }

        unsafe extern "C" fn create_offer_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to create offer: {:?}", error),
            }));
        }

        let observer = lkCreateSdpObserver {
            onSuccess: Some(create_offer_on_success),
            onFailure: Some(create_offer_on_failure),
        };

        unsafe {
            sys::lkCreateOffer(self.ffi.as_ptr(), &options.into(), &observer, userdata);
        }

        rx.recv().await.unwrap()
    }

    pub async fn create_answer(
        &self,
        options: AnswerOptions,
    ) -> Result<SessionDescription, RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<SessionDescription, RtcError>>(1);
        let tx_box = Box::new(tx);
        type CtxType = mpsc::Sender<Result<SessionDescription, RtcError>>;

        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn create_answer_on_success(
            desc: *mut lkSessionDescription,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Ok(SessionDescription {
                ffi: unsafe { sys::RefCounted::from_raw(desc) },
            }));
        }

        unsafe extern "C" fn create_answer_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to create answer: {:?}", (*error).message),
            }));
        }

        let observer = lkCreateSdpObserver {
            onSuccess: Some(create_answer_on_success),
            onFailure: Some(create_answer_on_failure),
        };

        unsafe {
            sys::lkCreateAnswer(self.ffi.as_ptr(), &options.into(), &observer, userdata);
        }

        rx.recv().await.unwrap()
    }

    pub async fn set_local_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<(), RtcError>>(1);
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn set_local_desc_on_success(userdata: *mut std::ffi::c_void) {
            let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            let _ = tx.blocking_send(Ok(()));
            // Box is dropped here
        }
        unsafe extern "C" fn set_local_desc_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to set local description: {:?}", error),
            }));
            // Box is dropped here
        }
        let observer = lkSetSdpObserver {
            onSuccess: Some(set_local_desc_on_success),
            onFailure: Some(set_local_desc_on_failure),
        };

        unsafe {
            sys::lkSetLocalDescription(self.ffi.as_ptr(), desc.ffi.as_ptr(), &observer, userdata);
        }
        rx.recv().await.unwrap()
    }

    pub async fn set_remote_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<(), RtcError>>(1);
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;
        unsafe extern "C" fn set_remote_desc_on_success(userdata: *mut std::ffi::c_void) {
            let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            let _ = tx.blocking_send(Ok(()));
            // Box is dropped here
        }
        unsafe extern "C" fn set_remote_desc_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to set remote description: {:?}", error),
            }));
            // Box is dropped here
        }
        let observer = lkSetSdpObserver {
            onSuccess: Some(set_remote_desc_on_success),
            onFailure: Some(set_remote_desc_on_failure),
        };

        unsafe {
            sys::lkSetRemoteDescription(self.ffi.as_ptr(), desc.ffi.as_ptr(), &observer, userdata);
        }

        rx.recv().await.unwrap()
    }

    pub async fn add_ice_candidate(&self, candidate: IceCandidate) -> Result<(), RtcError> {
        let (tx, mut rx) = mpsc::channel::<Result<(), RtcError>>(1);
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn on_complete(
            error: *mut sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<mpsc::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            if error.is_null() {
                let _ = tx.blocking_send(Ok(()));
                return;
            }
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to add ICE candidate: {:?}", (*error).message),
            }));
        }

        unsafe {
            sys::lkAddIceCandidate(
                self.ffi.as_ptr(),
                candidate.ffi.as_ptr(),
                Some(on_complete),
                userdata,
            );
        }

        rx.recv().await.unwrap()
    }

    pub fn restart_ice(&self) {
        unsafe {
            sys::lkPeerRestartIce(self.ffi.as_ptr());
        }
    }

    pub fn connection_state(&self) -> PeerConnectionState {
        unsafe { sys::lkGetPeerState(self.ffi.as_ptr()).into() }
    }

    pub fn ice_connection_state(&self) -> IceConnectionState {
        unsafe { sys::lkPeerGetIceConnectionState(self.ffi.as_ptr()).into() }
    }

    pub fn ice_gathering_state(&self) -> IceGatheringState {
        unsafe { sys::lkPeerGetIceGatheringState(self.ffi.as_ptr()).into() }
    }

    pub fn signaling_state(&self) -> SignalingState {
        unsafe { sys::lkPeerGetSignalingState(self.ffi.as_ptr()).into() }
    }

    pub fn current_local_description(&self) -> Option<SessionDescription> {
        unsafe {
            let desc_ptr = sys::lkPeerGetCurrentLocalDescription(self.ffi.as_ptr());
            if desc_ptr.is_null() {
                return None;
            }

            Some(SessionDescription { ffi: sys::RefCounted::from_raw(desc_ptr as *mut _) })
        }
    }

    pub fn current_remote_description(&self) -> Option<SessionDescription> {
        unsafe {
            let desc_ptr = sys::lkPeerGetCurrentRemoteDescription(self.ffi.as_ptr());
            if desc_ptr.is_null() {
                return None;
            }

            Some(SessionDescription { ffi: sys::RefCounted::from_raw(desc_ptr as *mut _) })
        }
    }

    pub fn close(&self) {
        unsafe {
            sys::lkPeerClose(self.ffi.as_ptr());
        }
    }

    pub fn create_data_channel(
        &self,
        label: &str,
        init: DataChannelInit,
    ) -> Result<DataChannel, RtcError> {
        let ffi = unsafe {
            sys::lkCreateDataChannel(
                self.ffi.as_ptr(),
                std::ffi::CString::new(label).unwrap().as_ptr(),
                &init.into(),
            )
        };

        if ffi.is_null() {
            return Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to create data channel"),
            });
        }

        let dc = DataChannel::configure(unsafe { sys::RefCounted::from_raw(ffi) });

        Ok(dc)
    }

    pub fn observer(&self) -> Arc<Mutex<PeerObserver>> {
        self.observer.clone()
    }

    pub fn add_track(
        &self,
        track: MediaStreamTrack,
        stream_ids: &Vec<String>,
    ) -> Result<RtpSender, RtcError> {
        let mut lk_err = sys::lkRtcError { message: std::ptr::null() };
        unsafe {
            let rtp_sender = sys::lkPeerAddTrack(
                self.ffi.as_ptr(),
                track.ffi().as_ptr(),
                stream_ids
                    .iter()
                    .map(|s| std::ffi::CString::new(s.as_str()).unwrap())
                    .collect::<Vec<std::ffi::CString>>()
                    .iter()
                    .map(|s| s.as_ptr())
                    .collect::<Vec<*const std::os::raw::c_char>>()
                    .as_ptr() as *mut _,
                stream_ids.len() as i32,
                &mut lk_err,
            );

            if !lk_err.message.is_null() {
                return Err(RtcError {
                    error_type: RtcErrorType::Internal,
                    message: format!(
                        "Failed to add track: {}",
                        std::ffi::CStr::from_ptr(lk_err.message).to_str().unwrap()
                    ),
                });
            }

            Ok(RtpSender { ffi: sys::RefCounted::from_raw(rtp_sender) })
        }
    }

    pub fn remove_track(&self, sender: RtpSender) -> Result<(), RtcError> {
        let mut lk_err = sys::lkRtcError { message: std::ptr::null() };
        unsafe {
            sys::lkPeerRemoveTrack(self.ffi.as_ptr(), sender.ffi.as_ptr(), &mut lk_err);

            if !lk_err.message.is_null() {
                return Err(RtcError {
                    error_type: RtcErrorType::Internal,
                    message: format!(
                        "Failed to remove track: {}",
                        std::ffi::CStr::from_ptr(lk_err.message).to_str().unwrap()
                    ),
                });
            }

            Ok(())
        }
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        todo!("get_stats is not yet implemented")
    }

    pub fn add_transceiver(
        &self,
        track: MediaStreamTrack,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        unsafe {
            let lk_init = sys::RtpTransceiverInitToNative(init);

            let mut rtc_err = sys::lkRtcError { message: std::ptr::null() };

            let lk_transceiver = sys::lkPeerAddTransceiver(
                self.ffi.as_ptr(),
                track.ffi().clone().as_ptr(),
                lk_init.as_ptr(),
                &mut rtc_err,
            );

            if lk_transceiver.is_null() {
                return Err(RtcError {
                    error_type: crate::RtcErrorType::Internal,
                    message: "add_transceiver failed".to_owned(),
                });
            }

            Ok(RtpTransceiver { ffi: sys::RefCounted::from_raw(lk_transceiver) })
        }
    }

    pub fn add_transceiver_for_media(
        &self,
        media_type: MediaType,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        unsafe {
            let lk_init = sys::RtpTransceiverInitToNative(init);

            let mut rtc_err = sys::lkRtcError { message: std::ptr::null() };

            let lk_transceiver = sys::lkPeerAddTransceiverForMedia(
                self.ffi.as_ptr(),
                media_type.into(),
                lk_init.as_ptr(),
                &mut rtc_err,
            );

            if lk_transceiver.is_null() {
                return Err(RtcError {
                    error_type: crate::RtcErrorType::Internal,
                    message: "add_transceiver_for_media failed".to_owned(),
                });
            }

            Ok(RtpTransceiver { ffi: sys::RefCounted::from_raw(lk_transceiver) })
        }
    }

    pub fn senders(&self) -> Vec<RtpSender> {
        let lk_vec = unsafe { sys::lkPeerGetSenders(self.ffi.as_ptr()) };
        let item_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if item_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut items = Vec::new();
        for i in 0..item_ptrs.vec.len() as isize {
            items.push(RtpSender { ffi: item_ptrs.vec[i as usize].clone() });
        }
        items
    }

    pub fn receivers(&self) -> Vec<RtpReceiver> {
        let lk_vec = unsafe { sys::lkPeerGetReceivers(self.ffi.as_ptr()) };
        let item_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if item_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut items = Vec::new();
        for i in 0..item_ptrs.vec.len() as isize {
            items.push(RtpReceiver { ffi: item_ptrs.vec[i as usize].clone() });
        }
        items
    }

    pub fn transceivers(&self) -> Vec<RtpTransceiver> {
        let lk_vec = unsafe { sys::lkPeerGetTransceivers(self.ffi.as_ptr()) };
        let item_ptrs = sys::RefCountedVector::from_native_vec(lk_vec);
        if item_ptrs.vec.is_empty() {
            return Vec::new();
        }
        let mut items = Vec::new();
        for i in 0..item_ptrs.vec.len() as isize {
            items.push(RtpTransceiver { ffi: item_ptrs.vec[i as usize].clone() });
        }
        items
    }

    pub fn on_connection_state_change(&self, f: Option<OnConnectionChange>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.connection_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_data_channel(&self, f: Option<OnDataChannel>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.data_channel_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_candidate(&self, f: Option<OnIceCandidate>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.ice_candidate_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_candidate_error(&self, f: Option<OnIceCandidateError>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.ice_candidate_error_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_connection_state_change(&self, f: Option<OnIceConnectionChange>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.ice_connection_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_gathering_state_change(&self, f: Option<OnIceGatheringChange>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.ice_gathering_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_negotiation_needed(&self, f: Option<OnNegotiationNeeded>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.negotiation_needed_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_signaling_state_change(&self, f: Option<OnSignalingChange>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.signaling_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_track(&self, f: Option<OnTrack>) {
        let binding = self.observer.lock().unwrap();
        let mut guard = binding.track_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }
}

impl Debug for PeerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerConnection")
            .field("state", &self.connection_state())
            .field("ice_state", &self.ice_connection_state())
            .finish()
    }
}

pub static PEER_OBSERVER: sys::lkPeerObserver = sys::lkPeerObserver {
    onSignalingChange: Some(PeerObserver::peer_on_signaling_change),
    onIceCandidate: Some(PeerObserver::peer_on_ice_candidate),
    onDataChannel: Some(PeerObserver::peer_on_data_channel),
    onTrack: Some(PeerObserver::peer_on_track),
    onRemoveTrack: Some(PeerObserver::peer_on_remove_track),
    onConnectionChange: Some(PeerObserver::peer_on_connection_state_change),
    onStandardizedIceConnectionChange: Some(
        PeerObserver::peer_on_standardized_ice_connection_change,
    ),
    onIceCandidateError: Some(PeerObserver::peer_on_ice_candidate_error),
    onRenegotiationNeeded: Some(PeerObserver::peer_on_renegotiation_needed),
    onIceGatheringChange: Some(PeerObserver::peer_on_ice_gathering_change),
};

#[cfg(test)]
mod tests {

    use crate::{data_channel::DataChannelInit, peer_connection::*, peer_connection_factory::*};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn create_pc() {
        let _ = env_logger::builder().is_test(true).try_init();

        let factory = PeerConnectionFactory::default();
        let config = RtcConfiguration {
            ice_servers: vec![IceServer {
                urls: vec!["stun:stun1.l.google.com:19302".to_string()],
                username: "".into(),
                password: "".into(),
            }],
            continual_gathering_policy: ContinualGatheringPolicy::GatherOnce,
            ice_transport_type: IceTransportsType::All,
        };

        let bob = factory.create_peer_connection(config.clone()).unwrap();

        bob.on_signaling_state_change(Some(Box::new(move |state| {
            println!("Bob signaling state changed: {:?}", state);
        })));

        let alice = factory.create_peer_connection(config.clone()).unwrap();

        let (bob_ice_tx, mut bob_ice_rx) = mpsc::unbounded_channel::<IceCandidate>();
        let (alice_ice_tx, mut alice_ice_rx) = mpsc::unbounded_channel::<IceCandidate>();
        let (alice_dc_tx, mut alice_dc_rx) = mpsc::unbounded_channel::<DataChannel>();

        bob.on_ice_candidate(Some(Box::new(move |candidate| {
            bob_ice_tx.send(candidate).unwrap();
        })));

        alice.on_ice_candidate(Some(Box::new(move |candidate| {
            alice_ice_tx.send(candidate).unwrap();
        })));

        alice.on_data_channel(Some(Box::new(move |dc: DataChannel| {
            dc.on_state_change(Some(Box::new(move |state| {
                println!("Alice data channel state changed: {:?}", state);
            })));
            println!("Alice received data channel: {:?}", dc.label());
            alice_dc_tx.send(dc).unwrap();
        })));

        let bob_dc = bob.create_data_channel("test_dc", DataChannelInit::default()).unwrap();
        bob_dc.on_state_change(Some(Box::new(move |state| {
            println!("Bob data channel state changed: {:?}", state);
        })));
        let offer = bob.create_offer(OfferOptions::default()).await.unwrap();
        println!("Bob offer: {:?}", offer.sdp());

        bob.set_local_description(offer.clone()).await.unwrap();
        alice.set_remote_description(offer).await.unwrap();
        let answer = alice.create_answer(AnswerOptions::default()).await.unwrap();
        println!("Alice answer: {:?}", answer.sdp());
        alice.set_local_description(answer.clone()).await.unwrap();
        bob.set_remote_description(answer).await.unwrap();

        let bob_ice = bob_ice_rx.recv().await.unwrap();
        println!("Bob ICE candidate: {:?}", bob_ice.candidate());
        let alice_ice = alice_ice_rx.recv().await.unwrap();
        println!("Alice ICE candidate: {:?}", alice_ice.candidate());

        bob.add_ice_candidate(alice_ice).await.unwrap();
        alice.add_ice_candidate(bob_ice).await.unwrap();

        let current_local_sdp = bob.current_local_description().unwrap();
        println!("Bob current local SDP: {:?}", current_local_sdp.sdp());

        let current_remote_sdp = bob.current_remote_description().unwrap();
        println!("Bob current remote SDP: {:?}", current_remote_sdp.sdp());

        let current_connection_state = bob.connection_state();
        println!("Bob current connection state: {:?}", current_connection_state);

        let current_ice_state = bob.ice_connection_state();
        println!("Bob current ICE connection state: {:?}", current_ice_state);

        let ice_gathering_state = bob.ice_gathering_state();
        println!("Bob current ICE gathering state: {:?}", ice_gathering_state);

        let signaling_state = bob.signaling_state();
        println!("Bob current signaling state: {:?}", signaling_state);

        let (data_tx, mut data_rx) = mpsc::unbounded_channel::<String>();
        let alice_dc = alice_dc_rx.recv().await.unwrap();

        alice_dc.on_message(Some(Box::new(move |buffer| {
            println!("Alice received data: {:?}", String::from_utf8_lossy(buffer.data).to_string());
            data_tx.send(String::from_utf8_lossy(buffer.data).to_string()).unwrap();
        })));

        bob_dc.send_async(b"This is a test", true).await.unwrap();
        assert_eq!(data_rx.recv().await.unwrap(), "This is a test");

        alice.close();
        bob.close();
    }
}
