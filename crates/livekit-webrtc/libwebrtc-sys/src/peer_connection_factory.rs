use std::any::Any;
use std::thread::sleep;
use std::time::Duration;
use cxx::UniquePtr;
use log::info;
use crate::candidate::ffi::Candidate;
use crate::data_channel::ffi::DataChannel;
use crate::jsep::ffi::IceCandidate;
use crate::media_stream_interface::ffi::MediaStreamInterface;
use crate::{jsep, peer_connection};
use crate::jsep::CreateSdpObserver;
use crate::peer_connection::ffi::{CandidatePairChangeEvent, IceConnectionState, IceGatheringState, PeerConnectionState, SignalingState};
use crate::peer_connection::PeerConnectionObserver;
use crate::rtp_receiver::ffi::RtpReceiver;
use crate::rtp_transceiver::ffi::RtpTransceiver;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug, Clone)]
    pub struct ICEServer {
        urls: Vec<String>,
        username: String,
        password: String,
    }

    #[derive(Debug)]
    pub struct RTCConfiguration {
        pub ice_servers: Vec<ICEServer>,
    }

    unsafe extern "C++" {
        include!("livekit/peer_connection_factory.h");

        type PeerConnection = crate::peer_connection::ffi::PeerConnection;
        type NativePeerConnectionObserver = crate::peer_connection::ffi::NativePeerConnectionObserver;
        type PeerConnectionFactory;
        type NativeRTCConfiguration;

        fn create_peer_connection_factory() -> UniquePtr<PeerConnectionFactory>;
        fn create_rtc_configuration(conf: RTCConfiguration) -> UniquePtr<NativeRTCConfiguration>;

        unsafe fn create_peer_connection(self: &PeerConnectionFactory, config: UniquePtr<NativeRTCConfiguration>, observer: UniquePtr<NativePeerConnectionObserver>) -> Result<UniquePtr<PeerConnection>>;
    }
  }







/*






#[cfg(test)]
mod test {

    struct TestObserver {

    }

    impl PeerConnectionObserver for TestObserver {
        fn on_signaling_change(&self, new_state: SignalingState) {
            log::debug!("Signaling state changed: {:?}", new_state);
        }

        fn on_add_stream(&self, stream: UniquePtr<MediaStreamInterface>) {
            todo!()
        }

        fn on_remove_stream(&self, stream: UniquePtr<MediaStreamInterface>) {
            todo!()
        }

        fn on_data_channel(&self, data_channel: UniquePtr<DataChannel>) {
            todo!()
        }

        fn on_renegotiation_needed(&self) {
            todo!()
        }

        fn on_negotiation_needed_event(&self, event: u32) {
            todo!()
        }

        fn on_ice_connection_change(&self, new_state: IceConnectionState) {
            log::debug!("ICE connection state changed: {:?}", new_state);
        }

        fn on_standardized_ice_connection_change(&self, new_state: IceConnectionState) {
            todo!()
        }

        fn on_connection_change(&self, new_state: PeerConnectionState) {
            log::debug!("PeerConnection state changed: {:?}", new_state);
        }

        fn on_ice_gathering_change(&self, new_state: IceGatheringState) {
            todo!()
        }

        fn on_ice_candidate(&self, candidate: UniquePtr<IceCandidate>) {
            todo!()
        }

        fn on_ice_candidate_error(&self, address: String, port: i32, url: String, error_code: i32, error_text: String) {
            todo!()
        }

        fn on_ice_candidates_removed(&self, removed: Vec<UniquePtr<Candidate>>) {
            todo!()
        }

        fn on_ice_connection_receiving_change(&self, receiving: bool) {
            todo!()
        }

        fn on_ice_selected_candidate_pair_changed(&self, event: CandidatePairChangeEvent) {
            todo!()
        }

        fn on_add_track(&self, receiver: UniquePtr<RtpReceiver>, streams: Vec<UniquePtr<MediaStreamInterface>>) {
            todo!()
        }

        fn on_track(&self, transceiver: UniquePtr<RtpTransceiver>) {
            todo!()
        }

        fn on_remove_track(&self, receiver: UniquePtr<RtpReceiver>) {
            todo!()
        }

        fn on_interesting_usage(&self, usage_pattern: i32) {
            todo!()
        }
    }

    struct SessionObserver {

    }

    impl CreateSdpObserver for SessionObserver {
        fn on_success(&self, session_description: UniquePtr<crate::jsep::ffi::SessionDescription>) {
            info!("on_success");
        }

        fn on_failure(&self, error: UniquePtr<crate::rtc_error::ffi::RTCError>) {
            info!("on_failure");
        }
    }

    #[test]
    fn create_pc_test() {
        env_logger::init();
        let factory = ffi::create_peer_connection_factory(); // Default factory config is defined on the c++ side atm
        unsafe {
            let mut pc = factory.create_peer_connection(ffi::create_rtc_configuration(ffi::RTCConfiguration {
                ice_servers: vec![ffi::ICEServer {
                    urls: vec!["stun:stun.l.google.com:19302".to_string()],
                    username: "".to_string(),
                    password: "".to_string(),
                }],
            }),  peer_connection::ffi::create_native_peer_connection_observer(Box::new(peer_connection::PeerConnectionObserverWrapper::new(Box::new(TestObserver{}))))).unwrap();


            let options = peer_connection::ffi::RTCOfferAnswerOptions::default();

            let sdp_observer = jsep::ffi::create_native_create_sdp_observer(Box::new(jsep::CreateSdpObserverWrapper::new(Box::new(SessionObserver{}))));
            pc.pin_mut().create_offer(sdp_observer, options);

            sleep(Duration::from_secs(2));

            pc.pin_mut().close();
        }
    }
}

*/

