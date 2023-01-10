use livekit_webrtc::peer_connection_factory::PeerConnectionFactory;
use livekit_webrtc::webrtc::RTCRuntime;
use std::fmt::{Debug, Formatter};
use tracing::trace;

/// SAFETY: The order of initialization and deletion is important for LKRuntime.
/// See the C++ constructors & destructors of these fields

pub struct LKRuntime {
    pub pc_factory: PeerConnectionFactory,
    pub rtc_runtime: RTCRuntime,
}

impl Debug for LKRuntime {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "LKRuntime")
    }
}

impl Default for LKRuntime {
    fn default() -> Self {
        trace!("LKRuntime::default()");
        let rtc_runtime = RTCRuntime::new();
        Self {
            pc_factory: PeerConnectionFactory::new(rtc_runtime.clone()),
            rtc_runtime,
        }
    }
}

impl Drop for LKRuntime {
    fn drop(&mut self) {
        trace!("LKRuntime::drop()");
    }
}
