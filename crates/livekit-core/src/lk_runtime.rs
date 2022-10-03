use std::fmt::{Debug, Formatter};

use tracing::{event, Level};

use livekit_webrtc::peer_connection_factory::PeerConnectionFactory;
use livekit_webrtc::webrtc::RTCRuntime;

/// SAFETY: The order of initialization and deletion is important for LKRuntime.
/// See the C++ constructors & destructor of these fields

pub struct LKRuntime {
    pub pc_factory: PeerConnectionFactory,
    pub rtc_runtime: RTCRuntime,
}

impl Debug for LKRuntime {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "LKRuntime")
    }
}

impl LKRuntime {
    pub fn new() -> Self {
        event!(Level::TRACE, "LKRuntime::new()");
        let rtc_runtime = RTCRuntime::new();
        Self {
            pc_factory: PeerConnectionFactory::new(rtc_runtime.clone()),
            rtc_runtime,
        }
    }
}

impl Drop for LKRuntime {
    fn drop(&mut self) {
        event!(Level::TRACE, "LKRuntime::drop()");
    }
}
