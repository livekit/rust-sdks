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

    // ===== Device Management Methods =====
    // These methods are internal - used by PlatformAudio. Use PlatformAudio for the public API.

    /// Get the number of playout (output) devices
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn playout_devices(&self) -> i16 {
        self.pc_factory.playout_devices()
    }

    /// Get the number of recording (input) devices
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn recording_devices(&self) -> i16 {
        self.pc_factory.recording_devices()
    }

    /// Get the name of a playout device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn playout_device_name(&self, index: u16) -> String {
        self.pc_factory.playout_device_name(index)
    }

    /// Get the name of a recording device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn recording_device_name(&self, index: u16) -> String {
        self.pc_factory.recording_device_name(index)
    }

    /// Get the GUID of a playout device by index.
    /// The GUID is a platform-specific unique identifier that is stable across device hot-plug events.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn playout_device_guid(&self, index: u16) -> String {
        self.pc_factory.playout_device_guid(index)
    }

    /// Get the GUID of a recording device by index.
    /// The GUID is a platform-specific unique identifier that is stable across device hot-plug events.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn recording_device_guid(&self, index: u16) -> String {
        self.pc_factory.recording_device_guid(index)
    }

    /// Set the playout device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_playout_device(&self, index: u16) -> bool {
        self.pc_factory.set_playout_device(index)
    }

    /// Set the recording device by index
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_recording_device(&self, index: u16) -> bool {
        self.pc_factory.set_recording_device(index)
    }

    /// Set the playout device by GUID.
    /// This is preferred over index as GUIDs are stable across device hot-plug events.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_playout_device_by_guid(&self, guid: &str) -> bool {
        self.pc_factory.set_playout_device_by_guid(guid)
    }

    /// Set the recording device by GUID.
    /// This is preferred over index as GUIDs are stable across device hot-plug events.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_recording_device_by_guid(&self, guid: &str) -> bool {
        self.pc_factory.set_recording_device_by_guid(guid)
    }

    /// Stop recording (clears initialized state, allowing device switch)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn stop_recording(&self) -> bool {
        self.pc_factory.stop_recording()
    }

    /// Initialize recording
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn init_recording(&self) -> bool {
        self.pc_factory.init_recording()
    }

    /// Start recording
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start_recording(&self) -> bool {
        self.pc_factory.start_recording()
    }

    /// Check if recording is initialized
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn recording_is_initialized(&self) -> bool {
        self.pc_factory.recording_is_initialized()
    }

    /// Stop playout (clears initialized state, allowing device switch)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn stop_playout(&self) -> bool {
        self.pc_factory.stop_playout()
    }

    /// Initialize playout
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn init_playout(&self) -> bool {
        self.pc_factory.init_playout()
    }

    /// Start playout
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn start_playout(&self) -> bool {
        self.pc_factory.start_playout()
    }

    /// Check if playout is initialized
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn playout_is_initialized(&self) -> bool {
        self.pc_factory.playout_is_initialized()
    }

    // ===== Built-in Audio Processing Methods =====
    // These methods are internal - used by PlatformAudio. Use PlatformAudio for the public API.

    /// Check if built-in (hardware) AEC is available
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn builtin_aec_is_available(&self) -> bool {
        self.pc_factory.builtin_aec_is_available()
    }

    /// Check if built-in (hardware) AGC is available
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn builtin_agc_is_available(&self) -> bool {
        self.pc_factory.builtin_agc_is_available()
    }

    /// Check if built-in (hardware) NS is available
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn builtin_ns_is_available(&self) -> bool {
        self.pc_factory.builtin_ns_is_available()
    }

    /// Enable or disable built-in (hardware) AEC
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn enable_builtin_aec(&self, enable: bool) -> bool {
        self.pc_factory.enable_builtin_aec(enable)
    }

    /// Enable or disable built-in (hardware) AGC
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn enable_builtin_agc(&self, enable: bool) -> bool {
        self.pc_factory.enable_builtin_agc(enable)
    }

    /// Enable or disable built-in (hardware) NS
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn enable_builtin_ns(&self, enable: bool) -> bool {
        self.pc_factory.enable_builtin_ns(enable)
    }

    /// Control whether ADM recording (microphone) is enabled.
    ///
    /// When disabled, WebRTC's calls to InitRecording/StartRecording will be no-ops.
    /// Use this when only using NativeAudioSource (no microphone capture needed).
    /// This prevents the microphone from interfering with the audio pipeline.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_adm_recording_enabled(&self, enabled: bool) {
        self.pc_factory.set_adm_recording_enabled(enabled)
    }

    /// Check if ADM recording (microphone) is enabled.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn adm_recording_enabled(&self) -> bool {
        self.pc_factory.adm_recording_enabled()
    }

    /// Control whether ADM playout (speakers) is enabled.
    ///
    /// When disabled (default), playout uses synthetic mode - remote audio is
    /// delivered via FFI callbacks to the application (e.g., Unity AudioSource).
    /// When enabled, remote audio plays through the platform speakers with AEC.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_adm_playout_enabled(&self, enabled: bool) {
        self.pc_factory.set_adm_playout_enabled(enabled)
    }

    /// Check if ADM playout (speakers) is enabled.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn adm_playout_enabled(&self) -> bool {
        self.pc_factory.adm_playout_enabled()
    }

    // ===== Platform ADM Lifecycle Management =====
    // These methods are internal - used by PlatformAudio. Use PlatformAudio for the public API.

    /// Acquires a reference to the Platform ADM.
    ///
    /// On first call, creates and initializes the Platform ADM. On subsequent
    /// calls, just increments the reference count.
    ///
    /// Returns true if Platform ADM is ready for use, false if initialization failed.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn acquire_platform_adm(&self) -> bool {
        self.pc_factory.acquire_platform_adm()
    }

    /// Releases a reference to the Platform ADM.
    ///
    /// When the reference count reaches zero, the Platform ADM is terminated
    /// and the proxy returns to synthetic mode.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn release_platform_adm(&self) {
        self.pc_factory.release_platform_adm()
    }

    /// Returns the current reference count for the Platform ADM.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn platform_adm_ref_count(&self) -> i32 {
        self.pc_factory.platform_adm_ref_count()
    }

    /// Returns true if Platform ADM is currently active (ref_count > 0).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn is_platform_adm_active(&self) -> bool {
        self.pc_factory.is_platform_adm_active()
    }
}

impl Drop for LkRuntime {
    fn drop(&mut self) {
        log::debug!("LkRuntime::drop()");
    }
}
