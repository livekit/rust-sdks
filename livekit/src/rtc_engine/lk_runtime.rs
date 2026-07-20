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
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Weak,
    },
    time::Duration,
};

use lazy_static::lazy_static;
use libwebrtc::prelude::*;
use parking_lot::{Condvar, Mutex};
use thiserror::Error;

#[cfg(not(target_arch = "wasm32"))]
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;

lazy_static! {
    static ref LK_RUNTIME: Mutex<LkRuntimeState> = Mutex::new(LkRuntimeState::default());

    /// Number of [`LkRuntime`] instances created but not yet fully dropped.
    ///
    /// Unlike [`LK_RUNTIME`] (whose [`Weak`] stops upgrading the instant the
    /// strong count hits zero, i.e. *before* `Drop` runs), this only reaches
    /// zero once teardown - peer connection factory destruction, ADM
    /// termination, capture/render worker-thread joins - has fully completed.
    static ref LK_RUNTIME_TEARDOWN_GATE: (Mutex<usize>, Condvar) =
        (Mutex::new(0), Condvar::new());
}

/// How long to wait for a previous runtime's teardown before giving up.
const RUNTIME_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(10);
static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Tracks a live [`LkRuntime`] in [`LK_RUNTIME_TEARDOWN_GATE`].
///
/// Must be the *last* field of [`LkRuntime`]: struct fields drop in
/// declaration order, so this guard's `Drop` (which opens the gate) only runs
/// after `pc_factory` has been fully destroyed.
struct RuntimeTeardownGuard {
    generation: u64,
}

impl RuntimeTeardownGuard {
    fn new(generation: u64) -> Self {
        *LK_RUNTIME_TEARDOWN_GATE.0.lock() += 1;
        Self { generation }
    }
}

impl Drop for RuntimeTeardownGuard {
    fn drop(&mut self) {
        let (lock, cv) = &*LK_RUNTIME_TEARDOWN_GATE;
        let mut live = lock.lock();
        *live = live.saturating_sub(1);
        log::debug!("LkRuntime generation {} teardown completed", self.generation);
        cv.notify_all();
    }
}

#[derive(Default)]
struct LkRuntimeState {
    runtime: Weak<LkRuntime>,
    zero_playout_delay: bool,
}

impl LkRuntimeState {
    fn enable_zero_playout_delay(
        &mut self,
        active_runtime_zero_playout_delay: Option<bool>,
    ) -> Result<(), WebRtcRuntimeInitializedError> {
        if active_runtime_zero_playout_delay == Some(false) {
            return Err(WebRtcRuntimeInitializedError);
        }

        self.zero_playout_delay = true;
        Ok(())
    }
}

/// Returned when zero playout delay is requested after the default WebRTC runtime is active.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("the WebRTC runtime is already initialized without zero playout delay")]
pub struct WebRtcRuntimeInitializedError;

pub struct LkRuntime {
    pc_factory: PeerConnectionFactory,
    zero_playout_delay: bool,
    active_rtc_sessions: Mutex<usize>,
    /// Keep last so it drops after `pc_factory`; see [`RuntimeTeardownGuard`].
    _teardown_guard: RuntimeTeardownGuard,
}

/// Keeps the runtime's audio transport alive while an RTC session can use it.
///
/// Dropping the last guard synchronously stops ADM workers and detaches their
/// callback before the final session's peer transports can be reclaimed.
pub(crate) struct ActiveRtcSessionGuard {
    runtime: Arc<LkRuntime>,
}

/// Keeps platform capture stopped while WebRTC's audio sender list mutates.
pub(crate) struct AudioCapturePauseGuard {
    runtime: Arc<LkRuntime>,
}

impl Drop for AudioCapturePauseGuard {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.runtime.pc_factory.resume_audio_capture();
    }
}

impl Drop for ActiveRtcSessionGuard {
    fn drop(&mut self) {
        let mut active_sessions = self.runtime.active_rtc_sessions.lock();
        debug_assert!(*active_sessions > 0);
        *active_sessions = active_sessions.saturating_sub(1);
        if *active_sessions == 0 {
            #[cfg(not(target_arch = "wasm32"))]
            self.runtime.shutdown_audio_io();
            log::debug!("last RTC session released; audio I/O shut down");
        }
    }
}

impl Debug for LkRuntime {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("LkRuntime").finish()
    }
}

impl LkRuntime {
    pub(crate) fn enable_zero_playout_delay() -> Result<(), WebRtcRuntimeInitializedError> {
        let mut state = LK_RUNTIME.lock();
        let active_runtime_zero_playout_delay =
            state.runtime.upgrade().map(|runtime| runtime.zero_playout_delay);
        state.enable_zero_playout_delay(active_runtime_zero_playout_delay)
    }

    pub fn instance() -> Arc<LkRuntime> {
        let mut state = LK_RUNTIME.lock();
        if let Some(lk_runtime) = state.runtime.upgrade() {
            lk_runtime
        } else {
            // The previous runtime's strong count may have just reached zero
            // while its `Drop` (factory + ADM teardown, including joining the
            // platform capture/render worker threads) is still running on
            // another thread. Creating a new factory/ADM concurrently with
            // that teardown races platform audio/device state, so wait for it
            // to finish first. Holding the `LK_RUNTIME` lock here is safe: the
            // dropping thread only touches the teardown gate, never this lock.
            if !Self::wait_for_teardown(RUNTIME_TEARDOWN_TIMEOUT) {
                log::error!(
                    "LkRuntime::instance() timed out after {RUNTIME_TEARDOWN_TIMEOUT:?} waiting \
                     for a previous runtime to tear down; continuing anyway - audio I/O from the \
                     old runtime may still be shutting down"
                );
            }
            let generation = NEXT_RUNTIME_GENERATION.fetch_add(1, Ordering::Relaxed);
            log::debug!("LkRuntime::new(generation={generation})");
            let zero_playout_delay = state.zero_playout_delay;
            #[cfg(not(target_arch = "wasm32"))]
            let pc_factory = if zero_playout_delay {
                PeerConnectionFactory::with_zero_playout_delay()
            } else {
                PeerConnectionFactory::default()
            };
            #[cfg(target_arch = "wasm32")]
            let pc_factory = PeerConnectionFactory::default();
            let new_runtime = Arc::new(Self {
                pc_factory,
                zero_playout_delay,
                active_rtc_sessions: Mutex::new(0),
                _teardown_guard: RuntimeTeardownGuard::new(generation),
            });
            state.runtime = Arc::downgrade(&new_runtime);
            new_runtime
        }
    }

    /// Blocks until every previously created runtime has finished tearing down,
    /// or `timeout` elapses.
    ///
    /// Returns `true` once no runtime teardown is in flight, `false` on
    /// timeout. Used by [`instance`](Self::instance) before constructing a new
    /// runtime, so that no factory/ADM teardown from a previous lifecycle
    /// overlaps the next one's startup.
    pub(crate) fn wait_for_teardown(timeout: Duration) -> bool {
        let (lock, cv) = &*LK_RUNTIME_TEARDOWN_GATE;
        let mut live = lock.lock();
        if *live == 0 {
            return true;
        }
        log::debug!("waiting for {} previous LkRuntime teardown(s) to complete", *live);
        !cv.wait_while_for(&mut live, |live| *live > 0, timeout).timed_out()
    }

    pub fn pc_factory(&self) -> &PeerConnectionFactory {
        &self.pc_factory
    }

    /// Registers an RTC session that may own the factory's audio transport.
    pub(crate) fn register_rtc_session(self: &Arc<Self>) -> ActiveRtcSessionGuard {
        let mut active_sessions = self.active_rtc_sessions.lock();
        *active_sessions += 1;
        log::debug!("registered RTC session (active={})", *active_sessions);
        ActiveRtcSessionGuard { runtime: self.clone() }
    }

    /// Stops capture until the returned guard is dropped.
    pub(crate) fn pause_audio_capture(self: &Arc<Self>) -> AudioCapturePauseGuard {
        #[cfg(not(target_arch = "wasm32"))]
        self.pc_factory.pause_audio_capture();
        AudioCapturePauseGuard { runtime: self.clone() }
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
    /// When the reference count reaches zero, platform audio I/O is stopped and
    /// the proxy returns to synthetic mode. The ADM remains available for a
    /// later acquire.
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

    /// Stops platform/synthetic audio I/O before runtime teardown.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn shutdown_audio_io(&self) {
        self.pc_factory.shutdown_audio_io();
    }
}

#[cfg(test)]
mod tests {
    use super::{LkRuntimeState, WebRtcRuntimeInitializedError};

    #[test]
    fn zero_playout_delay_can_be_enabled_early_and_repeated() {
        let mut state = LkRuntimeState::default();

        assert_eq!(state.enable_zero_playout_delay(None), Ok(()));
        assert!(state.zero_playout_delay);
        assert_eq!(state.enable_zero_playout_delay(Some(true)), Ok(()));
    }

    #[test]
    fn zero_playout_delay_rejects_late_enable_on_default_runtime() {
        let mut state = LkRuntimeState::default();

        assert_eq!(
            state.enable_zero_playout_delay(Some(false)),
            Err(WebRtcRuntimeInitializedError)
        );
        assert!(!state.zero_playout_delay);
    }
}

impl Drop for LkRuntime {
    fn drop(&mut self) {
        log::debug!("LkRuntime::drop()");
        #[cfg(not(target_arch = "wasm32"))]
        self.shutdown_audio_io();
    }
}
