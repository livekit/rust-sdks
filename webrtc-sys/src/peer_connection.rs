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
use tokio::sync::{mpsc, oneshot};

use crate::data_channel::{DataChannel, DataChannelInit};
use crate::ice_candidate::IceCandidate;
use crate::session_description::SessionDescription;
use crate::sys::{self, *};
use crate::{RtcError, RtcErrorType};

use crate::peer_connection_factory::RtcConfiguration;

/*
use crate::{
    data_channel::{DataChannel, DataChannelInit},
    ice_candidate::IceCandidate,
    imp::peer_connection as imp_pc,
    media_stream::MediaStream,
    media_stream_track::MediaStreamTrack,
    peer_connection_factory::RtcConfiguration,
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
    rtp_transceiver::{RtpTransceiver, RtpTransceiverInit},
    session_description::SessionDescription,
    stats::RtcStats,
    MediaType, RtcError,
};
*/

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

#[derive(Debug, Clone, Default)]
pub struct OfferOptions {
    pub ice_restart: bool,
    pub use_rtp_mux: bool,
    pub offer_to_receive_audio: bool,
    pub offer_to_receive_video: bool,
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
    // pub receiver: RtpReceiver,
    // pub streams: Vec<MediaStream>,
    // pub track: MediaStreamTrack,
    // pub transceiver: RtpTransceiver,
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
    pub fn lk_observer(&mut self) -> lkPeerObserver {
        lkPeerObserver {
            onSignalingChange: Some(Self::peer_on_signal_change),
            onIceCandidate: Some(Self::peer_on_ice_candidate),
            onDataChannel: Some(Self::peer_on_data_channel),
            onTrack: Some(Self::pee_on_track),
            onConnectionChange: Some(Self::peer_on_connection_state_change),
            onIceCandidateError: Some(Self::peer_on_ice_candidate_error),
        }
    }

    extern "C" fn peer_on_signal_change(state: lkSignalingState, userdata: *mut std::ffi::c_void) {
        let _peer: &mut PeerConnection = unsafe { &mut *userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.signaling_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    extern "C" fn peer_on_ice_candidate(
        sdp_mid: *const ::std::os::raw::c_char,
        sdp_mline_index: ::std::os::raw::c_int,
        candidate: *const ::std::os::raw::c_char,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let _peer: &mut PeerConnection = unsafe { &mut *userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.ice_candidate_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(IceCandidate {
                sdp_mid: unsafe { std::ffi::CStr::from_ptr(sdp_mid).to_str().unwrap().to_string() },
                sdp_mline_index,
                candidate: unsafe {
                    std::ffi::CStr::from_ptr(candidate).to_str().unwrap().to_string()
                },
            });
        }
    }

    #[allow(non_snake_case)]
    extern "C" fn peer_on_data_channel(dc: *const lkDataChannel, _userdata: *mut std::ffi::c_void) {
        let _peer: &mut PeerConnection = unsafe { &mut *_userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.data_channel_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            //TODO: create DataChannel from dc
            // f(DataChannel { handle: imp_dc::DataChannel::configure(dc)
            // });
        }
    }

    #[allow(non_snake_case)]
    extern "C" fn pee_on_track(
        transceiver: *const lkRtpTransceiver,
        _userdata: *mut std::ffi::c_void,
    ) {
        let _peer: &mut PeerConnection = unsafe { &mut *_userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.track_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            //TODO: create TrackEvent from transceiver
            println!("OnTrack: {:?}", transceiver);
        }
    }

    #[allow(non_snake_case)]
    extern "C" fn peer_on_connection_state_change(
        state: lkPeerState,
        _userdata: *mut std::ffi::c_void,
    ) {
        let _peer: &mut PeerConnection = unsafe { &mut *_userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.connection_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    #[allow(non_snake_case)]
    extern "C" fn peer_on_ice_candidate_error(
        address: *const ::std::os::raw::c_char,
        port: ::std::os::raw::c_int,
        url: *const ::std::os::raw::c_char,
        error_code: ::std::os::raw::c_int,
        error_text: *const ::std::os::raw::c_char,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let _peer: &mut PeerConnection = unsafe { &mut *userdata.cast::<PeerConnection>() };
        let mut handler = _peer.observer.ice_candidate_error_handler.lock().unwrap();
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
/*   fn lk_on_signaling_change(&self, new_state : lkSignalingState) {
    if let
      Some(f) = self.signaling_change_handler.lock().as_mut() {
        f(new_state.into());
      }
  }

  fn lk_on_data_channel(&self, data_channel : lkDataChannel) {
    if let
      Some(f) = self.data_channel_handler.lock().as_mut() {
        // f(DataChannel { handle: imp_dc::DataChannel::configure(data_channel)
        // });
      }
  }

  fn lk_on_renegotiation_needed(&self) {}

  fn lk_on_negotiation_needed_event(&self, event : u32) {
    if let
      Some(f) = self.negotiation_needed_handler.lock().as_mut() {
        f(event);
      }
  }

  fn lk_on_ice_connection_change(&self, _new_state : lkIceState) {}

  fn lk_on_standardized_ice_connection_change(&self, new_state : lkIceState) {
    if let
      Some(f) = self.ice_connection_change_handler.lock().as_mut() {
        f(new_state.into());
      }
  }

  fn lk_on_connection_change(&self, new_state : lkPeerState) {
    if let
      Some(f) = self.connection_change_handler.lock().as_mut() {
        f(new_state.into());
      }
  }

  fn lk_on_ice_gathering_change(&self, new_state : lkIceGatheringState) {
    if let
      Some(f) = self.ice_gathering_change_handler.lock().as_mut() {
        f(new_state.into());
      }
  }

  fn lk_on_ice_candidate(&self, cand : IceCandidate) {
    if let
      Some(f) = self.ice_candidate_handler.lock().as_mut() {
        f(cand);
      }
  }

  fn lk_on_ice_candidates_removed(&self, _removed : Vec<lkIceCandidate>, ) {}

  fn lk_on_ice_connection_receiving_change(&self, _receiving : bool) {}

  fn lk_on_ice_selected_candidate_pair_changed(
      &self, _event : CandidatePairChangeEvent, ) {}

  fn lk_on_add_track(&self, _receiver : lkRtpReceiver,
                     _streams : Vec<lkMediaStream>, ) {}

  fn lk_on_track(&self, transceiver : lkRtpTransceiver) {
    if let
      Some(f) = self.track_handler.lock().as_mut() {
        let receiver = transceiver.receiver();
        let streams = receiver.streams();
        let track = receiver.track();

        f(TrackEvent{
          receiver :
          RtpReceiver{handle : imp_rr::RtpReceiver{sys_handle : receiver}},
          streams : streams.into_iter()
              .map(
                  | s |
                  MediaStream{handle : imp_ms::MediaStream{sys_handle : s.ptr}})
              .collect(),
          track : imp_mst::new_media_stream_track(track),
          transceiver : RtpTransceiver{
            handle : imp_rt::RtpTransceiver{sys_handle : transceiver},
          },
        });
      }
  }

  fn lk_on_remove_track(
      &self,
      _receiver : SharedPtr<webrtc_sys::rtp_receiver::ffi::RtpReceiver>) {}

  fn lk_on_interesting_usage(&self, _usage_pattern : i32) {}
}
*/

#[cfg(test)]
mod tests {
    use log::trace;
    use tokio::sync::mpsc;

    use crate::{data_channel::DataChannelInit, peer_connection::*, peer_connection_factory::*};

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

        //alice.on_data_channel(Some(Box::new(move |dc| {
        //    alice_dc_tx.send(dc).unwrap();
        //})));

        let bob_dc = bob.create_data_channel("test_dc", DataChannelInit::default()).unwrap();

        let offer = bob.create_offer(OfferOptions::default()).await.unwrap();
        trace!("Bob offer: {:?}", offer);
        bob.set_local_description(offer.clone()).await.unwrap();
        alice.set_remote_description(offer).await.unwrap();

        let answer = alice.create_answer(AnswerOptions::default()).await.unwrap();
        trace!("Alice answer: {:?}", answer);
        alice.set_local_description(answer.clone()).await.unwrap();
        bob.set_remote_description(answer).await.unwrap();

        let bob_ice = bob_ice_rx.recv().await.unwrap();
        let alice_ice = alice_ice_rx.recv().await.unwrap();

        bob.add_ice_candidate(alice_ice).await.unwrap();
        alice.add_ice_candidate(bob_ice).await.unwrap();

        let (data_tx, mut data_rx) = mpsc::unbounded_channel::<String>();
        let alice_dc = alice_dc_rx.recv().await.unwrap();
        alice_dc.on_message(Some(Box::new(move |buffer| {
            data_tx.send(String::from_utf8_lossy(buffer.data).to_string()).unwrap();
        })));

        bob_dc.send(b"This is a test", true).unwrap();
        assert_eq!(data_rx.recv().await.unwrap(), "This is a test");

        alice.close();
        bob.close();
    }
}

#[derive(Clone)]
pub struct PeerConnection {
    pub(crate) observer: Arc<PeerObserver>,
    pub(crate) sys_peer: sys::RefCounted<sys::lkPeer>,
}

impl PeerConnection {
    pub fn set_configuration(&self, config: RtcConfiguration) -> Result<(), RtcError> {
        let sys_config: sys::lkRtcConfiguration = config.into();
        unsafe {
            sys::lkPeerSetConfig(self.sys_peer.as_ptr(), &sys_config);
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
            sdp_type: sys::lkSdpType,
            sdp: *const ::std::os::raw::c_char,
            userdata: *mut std::ffi::c_void,
        ) {
            let sdp = unsafe { std::ffi::CStr::from_ptr(sdp).to_str().unwrap() };
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Ok(SessionDescription {
                sdp_type: sdp_type.into(),
                sdp: sdp.to_string(),
            }));
        }

        unsafe extern "C" fn create_offer_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to create answer: {:?}", error),
            }));
        }

        let observer = lkCreateSdpObserver {
            onSuccess: Some(create_offer_on_success),
            onFailure: Some(create_offer_on_failure),
        };

        unsafe {
            if sys::lkCreateOffer(self.sys_peer.as_ptr(), &options.into(), &observer, userdata) {
                println!("lkCreateOffer called successfully");
            } else {
                println!("lkCreateOffer call failed");
            }
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

        // Prepare observer callbacks
        unsafe extern "C" fn create_answer_on_success(
            sdp_type: sys::lkSdpType,
            sdp: *const ::std::os::raw::c_char,
            userdata: *mut std::ffi::c_void,
        ) {
            let sdp = unsafe { std::ffi::CStr::from_ptr(sdp).to_str().unwrap() };
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Ok(SessionDescription {
                sdp_type: sdp_type.into(),
                sdp: sdp.to_string(),
            }));
        }

        unsafe extern "C" fn create_answer_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx = *Box::from_raw(userdata as *mut CtxType);
            let _ = tx.blocking_send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to create answer: {:?}", error),
            }));
        }

        let observer = lkCreateSdpObserver {
            onSuccess: Some(create_answer_on_success),
            onFailure: Some(create_answer_on_failure),
        };

        unsafe {
            if sys::lkCreateAnswer(self.sys_peer.as_ptr(), &options.into(), &observer, userdata) {
                println!("lkCreateAnswer called successfully");
            } else {
                println!("lkCreateAnswer call failed");
            }
        }

        rx.recv().await.unwrap()
    }

    pub async fn set_local_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let sdp_type = desc.sdp_type;
        let sdp = std::ffi::CString::new(desc.sdp.clone()).unwrap();

        let (tx, rx) = oneshot::channel::<Result<(), RtcError>>();
        // Box the sender and convert to a raw pointer
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        // Prepare observer callbacks (can be None if not needed)
        unsafe extern "C" fn set_local_desc_on_success(userdata: *mut std::ffi::c_void) {
            // Recover the Box and drop it after use
            let tx: Box<oneshot::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            println!("lkSetLocalDescription - OnSuccess");
            let _ = tx.send(Ok(()));
            // Box is dropped here
        }
        unsafe extern "C" fn set_local_desc_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<oneshot::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            println!("lkSetLocalDescription - OnFailure: {:?}", error);
            let _ = tx.send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to set remote description: {:?}", error),
            }));
            // Box is dropped here
        }
        let observer = lkSetSdpObserver {
            onSuccess: Some(set_local_desc_on_success),
            onFailure: Some(set_local_desc_on_failure),
        };

        unsafe {
            sys::lkSetLocalDescription(
                self.sys_peer.as_ptr(),
                sdp_type.into(),
                sdp.as_ptr(),
                &observer,
                userdata,
            );
        }

        rx.await.map_err(|_| RtcError {
            error_type: RtcErrorType::Internal,
            message: "set_local_description cancelled".to_owned(),
        })?
    }

    pub async fn set_remote_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        let sdp_type = desc.sdp_type;
        let sdp = std::ffi::CString::new(desc.sdp.clone()).unwrap();

        let (tx, rx) = oneshot::channel::<Result<(), RtcError>>();
        // Box the sender and convert to a raw pointer
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        // Prepare observer callbacks (can be None if not needed)
        unsafe extern "C" fn set_remote_desc_on_success(userdata: *mut std::ffi::c_void) {
            // Recover the Box and drop it after use
            let tx: Box<oneshot::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            println!("lkSetRemoteDescription - OnSuccess");
            let _ = tx.send(Ok(()));
            // Box is dropped here
        }
        unsafe extern "C" fn set_remote_desc_on_failure(
            error: *const sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<oneshot::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            println!("lkSetRemoteDescription - OnFailure: {:?}", error);
            let _ = tx.send(Err(RtcError {
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
            sys::lkSetRemoteDescription(
                self.sys_peer.as_ptr(),
                sdp_type.into(),
                sdp.as_ptr(),
                &observer,
                userdata,
            );
        }

        rx.await.map_err(|_| RtcError {
            error_type: RtcErrorType::Internal,
            message: "set_remote_description cancelled".to_owned(),
        })?
    }

    pub async fn add_ice_candidate(&self, candidate: IceCandidate) -> Result<(), RtcError> {
        let sdp_mid_cstr = std::ffi::CString::new(candidate.sdp_mid.clone()).unwrap();
        let candidate_cstr = std::ffi::CString::new(candidate.candidate.clone()).unwrap();

        let (tx, rx) = oneshot::channel::<Result<(), RtcError>>();
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn on_complete(
            error: *mut sys::lkRtcError,
            userdata: *mut std::ffi::c_void,
        ) {
            let tx: Box<oneshot::Sender<Result<(), RtcError>>> = Box::from_raw(userdata as *mut _);
            let _ = tx.send(Err(RtcError {
                error_type: RtcErrorType::Internal,
                message: format!("Failed to add ICE candidate: {:?}", error),
            }));
        }

        unsafe {
            if sys::lkAddIceCandidate(
                self.sys_peer.as_ptr(),
                sdp_mid_cstr.as_ptr(),
                candidate.sdp_mline_index,
                candidate_cstr.as_ptr(),
                Some(on_complete),
                userdata,
            ) {
                // If lkAddIceCandidate returns true, it means the candidate was added successfully
                let tx: Box<oneshot::Sender<Result<(), RtcError>>> =
                    Box::from_raw(userdata as *mut _);
                let _ = tx.send(Ok(()));
            }
        }

        rx.await.map_err(|_| RtcError {
            error_type: RtcErrorType::Internal,
            message: "add_ice_candidate cancelled".to_owned(),
        })?
    }

    pub fn close(&self) {
        unsafe {
            sys::lkPeerClose(self.sys_peer.as_ptr());
        }
    }

    pub fn create_data_channel(
        &self,
        label: &str,
        init: DataChannelInit,
    ) -> Result<DataChannel, RtcError> {
        Err(RtcError { error_type: RtcErrorType::Internal, message: format!("TODO: ") })
    }

    /*

    pub fn add_track<T: AsRef<str>>(
        &self,
        track: MediaStreamTrack,
        streams_ids: &[T],
    ) -> Result<RtpSender, RtcError> {
        self.sys_peer.add_track(track, streams_ids)
    }

    pub fn remove_track(&self, sender: RtpSender) -> Result<(), RtcError> {
        self.sys_peer.remove_track(sender)
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        self.sys_peer.get_stats().await
    }

    pub fn add_transceiver(
        &self,
        track: MediaStreamTrack,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        self.sys_peer.add_transceiver(track, init)
    }

    pub fn add_transceiver_for_media(
        &self,
        media_type: MediaType,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        self.sys_peer.add_transceiver_for_media(media_type, init)
    }



    pub fn restart_ice(&self) {
        self.sys_peer.restart_ice()
    }

    pub fn connection_state(&self) -> PeerConnectionState {
        self.sys_peer.connection_state()
    }

    pub fn ice_connection_state(&self) -> IceConnectionState {
        self.sys_peer.ice_connection_state()
    }

    pub fn ice_gathering_state(&self) -> IceGatheringState {
        self.sys_peer.ice_gathering_state()
    }

    pub fn signaling_state(&self) -> SignalingState {
        self.sys_peer.signaling_state()
    }

    pub fn current_local_description(&self) -> Option<SessionDescription> {
        self.sys_peer.current_local_description()
    }

    pub fn current_remote_description(&self) -> Option<SessionDescription> {
        self.sys_peer.current_remote_description()
    }

    pub fn senders(&self) -> Vec<RtpSender> {
        self.sys_peer.senders()
    }

    pub fn receivers(&self) -> Vec<RtpReceiver> {
        self.sys_peer.receivers()
    }

    pub fn transceivers(&self) -> Vec<RtpTransceiver> {
        self.sys_peer.transceivers()
    }

    pub fn observer(&self) -> Arc<PeerObserver> {
        self.observer.clone()
    }
    */
    pub fn on_connection_state_change(&self, f: Option<OnConnectionChange>) {
        let mut guard = self.observer.connection_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_data_channel(&self, f: Option<OnDataChannel>) {
        let mut guard = self.observer.data_channel_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_candidate(&self, f: Option<OnIceCandidate>) {
        let mut guard = self.observer.ice_candidate_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_candidate_error(&self, f: Option<OnIceCandidateError>) {
        let mut guard = self.observer.ice_candidate_error_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_connection_state_change(&self, f: Option<OnIceConnectionChange>) {
        let mut guard = self.observer.ice_connection_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_ice_gathering_state_change(&self, f: Option<OnIceGatheringChange>) {
        let mut guard = self.observer.ice_gathering_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_negotiation_needed(&self, f: Option<OnNegotiationNeeded>) {
        let mut guard = self.observer.negotiation_needed_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_signaling_state_change(&self, f: Option<OnSignalingChange>) {
        let mut guard = self.observer.signaling_change_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }

    pub fn on_track(&self, f: Option<OnTrack>) {
        let mut guard = self.observer.track_handler.lock().unwrap();
        guard.replace(f.unwrap());
    }
}

impl Debug for PeerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerConnection")
            //.field("state", &self.connection_state())
            //.field("ice_state", &self.ice_connection_state())
            .finish()
    }
}
