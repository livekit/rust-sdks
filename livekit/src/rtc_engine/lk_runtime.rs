use lazy_static::lazy_static;
use livekit_webrtc::prelude::*;
use parking_lot::Mutex;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Weak};

lazy_static! {
    static ref LK_RUNTIME: Mutex<Weak<LkRuntime>> = Mutex::new(Weak::new());
}

pub struct LkRuntime {
    pc_factory: PeerConnectionFactory,
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
            log::trace!("LkRuntime::new()");
            let new_runtime = Arc::new(Self {
                pc_factory: PeerConnectionFactory::default(),
            });
            *lk_runtime_ref = Arc::downgrade(&new_runtime);
            new_runtime
        }
    }

    pub fn pc_factory(&self) -> &PeerConnectionFactory {
        &self.pc_factory
    }
}

impl Drop for LkRuntime {
    fn drop(&mut self) {
        log::trace!("LkRuntime::drop()");
    }
}
