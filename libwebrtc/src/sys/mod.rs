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
mod tests {
    use super::*;

    // PeerObserver
    #[allow(non_snake_case)]
    extern "C" fn peerOnSignalChange(state: lkSignalingState, _userdata: *mut std::ffi::c_void) {
        println!("OnSignalChange: {:?}", state);
    }

    #[allow(non_snake_case)]
    extern "C" fn peerOnIceCandidate(
        candidate: *mut lkIceCandidate,
        _userdata: *mut ::std::os::raw::c_void,
    ) {
        println!("OnIceCandidate: {:?}", candidate);
    }

    #[allow(non_snake_case)]
    extern "C" fn peerOnDataChannel(dc: *const lkDataChannel, _userdata: *mut std::ffi::c_void) {
        println!("OnDataChannel: {:?}", dc);
    }

    #[allow(non_snake_case)]
    extern "C" fn peerOnTrack(
        transceiver: *const lkRtpTransceiver,
        _receiver: *const lkRtpReceiver,
        _streams: *const lkVectorGeneric,
        _track: *const lkMediaStreamTrack,
        _userdata: *mut std::ffi::c_void,
    ) {
        println!("OnTrack: {:?}", transceiver);
    }

    #[allow(non_snake_case)]
    extern "C" fn peerOnConnectionChange(state: lkPeerState, _userdata: *mut std::ffi::c_void) {
        println!("OnConnectionChange: {:?}", state);
    }

    #[allow(non_snake_case)]
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
    #[allow(non_snake_case)]
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

    #[allow(non_snake_case)]
    extern "C" fn createSdpOnFailure(error: *const lkRtcError, _userdata: *mut std::ffi::c_void) {
        println!("CreateSdp - OnFailure: {:?}", error);
    }

    // Set SDP observer
    #[allow(non_snake_case)]
    extern "C" fn setSdpOnSuccess(_userdata: *mut ::std::os::raw::c_void) {
        println!(" SetSDP - OnSuccess");
    }

    #[allow(non_snake_case)]
    extern "C" fn setSdpOnFailure(error: *const lkRtcError, _userdata: *mut std::ffi::c_void) {
        println!("SetSDP - OnFailure: {:?}", error);
    }

    #[allow(non_snake_case)]
    extern "C" fn onIceConnectionChange(state: lkIceState, _userdata: *mut ::std::os::raw::c_void) {
        println!("OnIceConnectionChange: {:?}", state);
    }

    #[allow(non_snake_case)]
    extern "C" fn onIceGatheringChange(
        state: lkIceGatheringState,
        _userdata: *mut ::std::os::raw::c_void,
    ) {
        println!("OnIceGatheringChange: {:?}", state);
    }

    #[allow(non_snake_case)]
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
