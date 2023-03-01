use livekit_webrtc::prelude::*;
use std::fmt::{Debug, Formatter};
use tracing::trace;

pub struct LkRuntime {
    pub pc_factory: PeerConnectionFactory,
}

impl Debug for LkRuntime {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "LKRuntime")
    }
}

impl Default for LkRuntime {
    fn default() -> Self {
        trace!("LkRuntime::default()");
        Self {
            pc_factory: PeerConnectionFactory::default(),
        }
    }
}

impl Drop for LkRuntime {
    fn drop(&mut self) {
        trace!("LkRuntime::drop()");
    }
}
