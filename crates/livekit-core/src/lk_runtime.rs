use log::trace;

use livekit_webrtc::peer_connection_factory::PeerConnectionFactory;
use livekit_webrtc::webrtc::RTCRuntime;

pub struct LKRuntime {
    pub rtc_runtime: RTCRuntime,
    pub pc_factory: PeerConnectionFactory,
}

impl LKRuntime {
    pub fn new() -> Self {
        trace!("LKRuntime::new()");
        Self {
            rtc_runtime: RTCRuntime::new(),
            pc_factory: PeerConnectionFactory::new(),
        }
    }
}

impl Drop for LKRuntime {
    fn drop(&mut self) {
        trace!("LKRuntime::drop()");
    }
}
