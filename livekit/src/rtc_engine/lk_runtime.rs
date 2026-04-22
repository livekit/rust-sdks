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

use std::{
    fmt::{Debug, Formatter},
    sync::{Arc, Weak},
};

use lazy_static::lazy_static;
use libwebrtc::prelude::*;
use parking_lot::Mutex;

#[cfg(not(target_arch = "wasm32"))]
use libwebrtc::native::AdmDelegateType;
#[cfg(not(target_arch = "wasm32"))]
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;

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
            log::debug!("LkRuntime::new()");
            let new_runtime = Arc::new(Self { pc_factory: PeerConnectionFactory::default() });
            *lk_runtime_ref = Arc::downgrade(&new_runtime);
            new_runtime
        }
    }

    pub fn pc_factory(&self) -> &PeerConnectionFactory {
        &self.pc_factory
    }

    // ===== ADM Management Methods =====
    // These methods allow runtime control of the Audio Device Module

    /// Enable platform ADM (WebRTC's built-in audio device management)
    ///
    /// When enabled, WebRTC handles audio device enumeration, selection,
    /// and audio capture/playout automatically.
    ///
    /// Note: This is an internal method used by FFI. Platform ADM is not
    /// exposed in the public Rust SDK.
    ///
    /// Returns true if platform ADM was successfully enabled.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_platform_adm(&self) -> bool {
        self.pc_factory.enable_platform_adm()
    }

    /// Clear ADM delegate, reverting to default behavior
    ///
    /// After calling this, you should use NativeAudioSource to push audio manually.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn clear_adm_delegate(&self) {
        self.pc_factory.clear_adm_delegate();
    }

    /// Get the current ADM delegate type
    #[cfg(not(target_arch = "wasm32"))]
    pub fn adm_delegate_type(&self) -> AdmDelegateType {
        self.pc_factory.adm_delegate_type()
    }

    /// Check if an ADM delegate is active
    #[cfg(not(target_arch = "wasm32"))]
    pub fn has_adm_delegate(&self) -> bool {
        self.pc_factory.has_adm_delegate()
    }

    /// Get the number of playout (output) devices
    #[cfg(not(target_arch = "wasm32"))]
    pub fn playout_devices(&self) -> i16 {
        self.pc_factory.playout_devices()
    }

    /// Get the number of recording (input) devices
    #[cfg(not(target_arch = "wasm32"))]
    pub fn recording_devices(&self) -> i16 {
        self.pc_factory.recording_devices()
    }

    /// Get the name of a playout device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub fn playout_device_name(&self, index: u16) -> String {
        self.pc_factory.playout_device_name(index)
    }

    /// Get the name of a recording device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub fn recording_device_name(&self, index: u16) -> String {
        self.pc_factory.recording_device_name(index)
    }

    /// Set the playout device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_playout_device(&self, index: u16) -> i32 {
        self.pc_factory.set_playout_device(index)
    }

    /// Set the recording device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_recording_device(&self, index: u16) -> i32 {
        self.pc_factory.set_recording_device(index)
    }

    /// Stop recording (clears initialized state, allowing device switch)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stop_recording(&self) -> i32 {
        self.pc_factory.stop_recording()
    }

    /// Initialize recording
    #[cfg(not(target_arch = "wasm32"))]
    pub fn init_recording(&self) -> i32 {
        self.pc_factory.init_recording()
    }

    /// Start recording
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_recording(&self) -> i32 {
        self.pc_factory.start_recording()
    }

    /// Check if recording is initialized
    #[cfg(not(target_arch = "wasm32"))]
    pub fn recording_is_initialized(&self) -> bool {
        self.pc_factory.recording_is_initialized()
    }

    /// Stop playout (clears initialized state, allowing device switch)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stop_playout(&self) -> i32 {
        self.pc_factory.stop_playout()
    }

    /// Initialize playout
    #[cfg(not(target_arch = "wasm32"))]
    pub fn init_playout(&self) -> i32 {
        self.pc_factory.init_playout()
    }

    /// Start playout
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_playout(&self) -> i32 {
        self.pc_factory.start_playout()
    }

    /// Check if playout is initialized
    #[cfg(not(target_arch = "wasm32"))]
    pub fn playout_is_initialized(&self) -> bool {
        self.pc_factory.playout_is_initialized()
    }
}

impl Drop for LkRuntime {
    fn drop(&mut self) {
        log::debug!("LkRuntime::drop()");
    }
}
