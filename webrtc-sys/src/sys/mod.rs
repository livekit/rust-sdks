#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(dead_code)]

mod conv;
mod ffi;
mod refcounted;

pub use ffi::*;
pub use refcounted::*;

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
        sdpMid: *const ::std::os::raw::c_char,
        sdpMLineIndex: ::std::os::raw::c_int,
        candidate: *const ::std::os::raw::c_char,
        userdata: *mut ::std::os::raw::c_void,
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
        sdpType: lkSdpType,
        sdp: *const ::std::os::raw::c_char,
        _userdata: *mut std::ffi::c_void,
    ) {
        let sdp = unsafe { std::ffi::CStr::from_ptr(sdp).to_str().unwrap() };

        println!("CreateSdp - OnSuccess: {:?} {:?}", sdpType, sdp);
    }

    #[allow(non_snake_case)]
    extern "C" fn createSdpOnFailure(error: *const lkRtcError, _userdata: *mut std::ffi::c_void) {
        println!("CreateSdp - OnFailure: {:?}", error);
    }

    #[test]
    fn test_dc_link() {
        unsafe {
            let observer = lkPeerObserver {
                onSignalingChange: Some(peerOnSignalChange),
                onIceCandidate: Some(peerOnIceCandidate),
                onDataChannel: Some(peerOnDataChannel),
                onTrack: Some(peerOnTrack),
                onConnectionChange: Some(peerOnConnectionChange),
                onIceCandidateError: Some(peerOnIceCandidateError),
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

            let offer_answer_options = lkOfferAnswerOptions { iceRestart: false, useRtpMux: true };
            assert!(lkCreateOffer(
                peer,
                &offer_answer_options,
                &create_sdp_observer,
                std::ptr::null_mut(),
            ));

            lkReleaseRef(peer);
            lkReleaseRef(factory);
            lkDispose();
        }
    }
}
