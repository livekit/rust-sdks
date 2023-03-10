use lazy_static::lazy_static;
use livekit_webrtc::prelude::*;
use parking_lot::Mutex;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Weak};
use tracing::trace;

lazy_static! {
    static ref LK_RUNTIME: Mutex<Weak<LkRuntime>> = Mutex::new(Weak::new());
}

pub struct LkRuntime {
    pub pc_factory: PeerConnectionFactory,
}

impl Debug for LkRuntime {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("LkRuntime").finish()
    }
}

impl LkRuntime {
    pub fn instance() -> Arc<LkRuntime> {
        let mut lk_runtime_ref = LK_RUNTIME.lock();
        if let Some(lk_runtime) = lk_runtime_ref.upgrade() {
            lk_runtime
        } else {
            let new_runtime = Arc::new(LkRuntime::default());
            *lk_runtime_ref = Arc::downgrade(&new_runtime);
            new_runtime
        }
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
