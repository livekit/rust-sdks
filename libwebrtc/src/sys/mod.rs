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

#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

mod conv;
mod ffi;
mod refcounted;
mod utils;

pub use conv::*;
pub use ffi::*;
pub use refcounted::*;
pub use utils::*;

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    // PeerObserver
    extern "C" fn peerOnSignalChange(state: lkSignalingState, _userdata: *mut std::ffi::c_void) {
        println!("OnSignalChange: {:?}", state);
    }

    extern "C" fn peerOnIceCandidate(
        candidate: *mut lkIceCandidate,
        _userdata: *mut ::std::os::raw::c_void,
    ) {
        println!("OnIceCandidate: {:?}", candidate);
    }

    extern "C" fn peerOnDataChannel(dc: *const lkDataChannel, _userdata: *mut std::ffi::c_void) {
        println!("OnDataChannel: {:?}", dc);
    }

    extern "C" fn peerOnTrack(
        transceiver: *const lkRtpTransceiver,
        _receiver: *const lkRtpReceiver,
        _streams: *const lkVectorGeneric,
        _track: *const lkMediaStreamTrack,
        _userdata: *mut std::ffi::c_void,
    ) {
        println!("OnTrack: {:?}", transceiver);
    }

    extern "C" fn peerOnConnectionChange(state: lkPeerState, _userdata: *mut std::ffi::c_void) {
        println!("OnConnectionChange: {:?}", state);
    }

    extern "C" fn peerOnIceCandidateError(
        address: *const ::std::os::raw::c_char,
        port: ::std::os::raw::c_int,
        url: *const ::std::os::raw::c_char,
        error_code: ::std::os::raw::c_int,
        error_text: *const ::std::os::raw::c_char,
        _userdata: *mut std::ffi::c_void,
    ) {
        println!(
            "OnIceCandidateError: {:?} {:?} {:?} {:?} {:?}",
            address, port, url, error_code, error_text
        );
    }

    // Create SDP observer
    extern "C" fn createSdpOnSuccess(
        desc: *mut lkSessionDescription,
        _userdata: *mut std::ffi::c_void,
    ) {
        println!("CreateSdp - OnSuccess: {:?} ", desc);
        let peer = _userdata as *mut lkPeer;
        let set_sdp_observer =
            lkSetSdpObserver { onSuccess: Some(setSdpOnSuccess), onFailure: Some(setSdpOnFailure) };
        unsafe {
            assert!(lkSetLocalDescription(peer, desc, &set_sdp_observer, std::ptr::null_mut()));
        }
    }

    extern "C" fn createSdpOnFailure(error: *const lkRtcError, _userdata: *mut std::ffi::c_void) {
        println!("CreateSdp - OnFailure: {:?}", error);
    }

    // Set SDP observer
    extern "C" fn setSdpOnSuccess(_userdata: *mut ::std::os::raw::c_void) {
        println!(" SetSDP - OnSuccess");
    }

    extern "C" fn setSdpOnFailure(error: *const lkRtcError, _userdata: *mut std::ffi::c_void) {
        println!("SetSDP - OnFailure: {:?}", error);
    }

    extern "C" fn onIceConnectionChange(state: lkIceState, _userdata: *mut ::std::os::raw::c_void) {
        println!("OnIceConnectionChange: {:?}", state);
    }

    extern "C" fn onIceGatheringChange(
        state: lkIceGatheringState,
        _userdata: *mut ::std::os::raw::c_void,
    ) {
        println!("OnIceGatheringChange: {:?}", state);
    }

    extern "C" fn onRenegotiationNeeded(_userdata: *mut ::std::os::raw::c_void) {
        println!("OnRenegotiationNeeded");
    }

    #[test]
    fn test_dc_link() {
        unsafe {
            let observer = lkPeerObserver {
                onSignalingChange: Some(peerOnSignalChange),
                onIceCandidate: Some(peerOnIceCandidate),
                onDataChannel: Some(peerOnDataChannel),
                onTrack: Some(peerOnTrack),
                onRemoveTrack: None,
                onConnectionChange: Some(peerOnConnectionChange),
                onIceCandidateError: Some(peerOnIceCandidateError),
                onStandardizedIceConnectionChange: Some(onIceConnectionChange),
                onIceGatheringChange: Some(onIceGatheringChange),
                onRenegotiationNeeded: Some(onRenegotiationNeeded),
            };

            let create_sdp_observer = lkCreateSdpObserver {
                onSuccess: Some(createSdpOnSuccess),
                onFailure: Some(createSdpOnFailure),
            };

            let rtc_config = lkRtcConfiguration {
                iceServers: std::ptr::null_mut(),
                iceServersCount: 0,
                iceTransportType: lkIceTransportType::LK_ICE_TRANSPORT_TYPE_ALL,
                gatheringPolicy: lkContinualGatheringPolicy::LK_GATHERING_POLICY_CONTINUALLY,
            };

            lkInitialize();
            let factory = lkCreatePeerFactory();
            let peer = lkCreatePeer(factory, &rtc_config, &observer, std::ptr::null_mut());

            let label = std::ffi::CString::new("test_data_channel").unwrap();
            let init = lkDataChannelInit { ordered: true, reliable: true, maxRetransmits: -1 };

            let _ = lkCreateDataChannel(peer, label.as_ptr(), &init);

            let offer_answer_options = lkOfferAnswerOptions {
                iceRestart: false,
                useRtpMux: true,
                offerToReceiveAudio: true,
                offerToReceiveVideo: true,
            };
            assert!(lkCreateOffer(
                peer,
                &offer_answer_options,
                &create_sdp_observer,
                peer as *mut ::std::os::raw::c_void,
            ));

            lkReleaseRef(peer);
            lkReleaseRef(factory);
            lkDispose();
        }
    }
}
