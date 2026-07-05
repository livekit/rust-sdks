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

use std::fmt::Debug;

use crate::{
    imp::peer_connection_factory as imp_pcf, peer_connection::PeerConnection,
    rtp_parameters::RtpCapabilities, MediaType, RtcError,
};

#[derive(Debug, Clone)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ContinualGatheringPolicy {
    GatherOnce,
    GatherContinually,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceTransportsType {
    Relay,
    NoHost,
    All,
}

#[derive(Debug, Clone)]
pub struct RtcConfiguration {
    pub ice_servers: Vec<IceServer>,
    pub continual_gathering_policy: ContinualGatheringPolicy,
    pub ice_transport_type: IceTransportsType,
}

impl Default for RtcConfiguration {
    fn default() -> Self {
        Self {
            ice_servers: vec![],
            continual_gathering_policy: ContinualGatheringPolicy::GatherContinually,
            ice_transport_type: IceTransportsType::All,
        }
    }
}

#[derive(Clone, Default)]
pub struct PeerConnectionFactory {
    pub(crate) handle: imp_pcf::PeerConnectionFactory,
}

impl Debug for PeerConnectionFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("PeerConnectionFactory").finish()
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        self.handle.create_peer_connection(config)
    }

    pub fn get_rtp_sender_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.handle.get_rtp_sender_capabilities(media_type)
    }

    pub fn get_rtp_receiver_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.handle.get_rtp_receiver_capabilities(media_type)
    }
}

pub mod native {
    use super::PeerConnectionFactory;
    use crate::{
        audio_source::native::NativeAudioSource, audio_track::RtcAudioTrack,
        video_source::native::NativeVideoSource, video_track::RtcVideoTrack,
    };

    pub trait PeerConnectionFactoryExt {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack;
        fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack;

        /// Create an audio track that uses the Platform ADM for capture.
        /// The track will capture audio from the selected recording device.
        fn create_device_audio_track(&self, label: &str) -> RtcAudioTrack;

        // Device enumeration
        fn playout_devices(&self) -> i16;
        fn recording_devices(&self) -> i16;
        fn playout_device_name(&self, index: u16) -> String;
        fn recording_device_name(&self, index: u16) -> String;
        /// Get device GUID (platform-specific unique identifier, stable across hot-plug)
        fn playout_device_guid(&self, index: u16) -> String;
        fn recording_device_guid(&self, index: u16) -> String;

        // Device selection by index
        fn set_playout_device(&self, index: u16) -> bool;
        fn set_recording_device(&self, index: u16) -> bool;
        /// Device selection by GUID (preferred - stable across device changes)
        fn set_playout_device_by_guid(&self, guid: &str) -> bool;
        fn set_recording_device_by_guid(&self, guid: &str) -> bool;

        // Recording control (for device switching while active)
        fn stop_recording(&self) -> bool;
        fn init_recording(&self) -> bool;
        fn start_recording(&self) -> bool;
        fn recording_is_initialized(&self) -> bool;

        // Playout control (for device switching while active)
        fn stop_playout(&self) -> bool;
        fn init_playout(&self) -> bool;
        fn start_playout(&self) -> bool;
        fn playout_is_initialized(&self) -> bool;

        // Built-in audio processing (hardware AEC/AGC/NS)
        // Only available on iOS and some Android devices
        fn builtin_aec_is_available(&self) -> bool;
        fn builtin_agc_is_available(&self) -> bool;
        fn builtin_ns_is_available(&self) -> bool;
        fn enable_builtin_aec(&self, enable: bool) -> bool;
        fn enable_builtin_agc(&self, enable: bool) -> bool;
        fn enable_builtin_ns(&self, enable: bool) -> bool;

        // ADM recording control
        // Use this to disable microphone when only using NativeAudioSource
        fn set_adm_recording_enabled(&self, enabled: bool);
        fn adm_recording_enabled(&self) -> bool;

        // ADM playout control
        // When disabled (default), playout uses synthetic mode - remote audio is
        // delivered via FFI callbacks. When enabled, plays through platform speakers.
        fn set_adm_playout_enabled(&self, enabled: bool);
        fn adm_playout_enabled(&self) -> bool;

        // Platform ADM lifecycle management
        // Call acquire_platform_adm when creating PlatformAudio.
        // Call release_platform_adm when disposing PlatformAudio.
        // The Platform ADM is only created when first acquired, and terminated
        // when the last reference is released.
        fn acquire_platform_adm(&self) -> bool;
        fn release_platform_adm(&self);
        fn platform_adm_ref_count(&self) -> i32;
        fn is_platform_adm_active(&self) -> bool;
    }

    impl PeerConnectionFactoryExt for PeerConnectionFactory {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack {
            self.handle.create_video_track(label, source)
        }

        fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack {
            self.handle.create_audio_track(label, source)
        }

        fn create_device_audio_track(&self, label: &str) -> RtcAudioTrack {
            self.handle.create_device_audio_track(label)
        }

        fn playout_devices(&self) -> i16 {
            self.handle.playout_devices()
        }

        fn recording_devices(&self) -> i16 {
            self.handle.recording_devices()
        }

        fn playout_device_name(&self, index: u16) -> String {
            self.handle.playout_device_name(index)
        }

        fn recording_device_name(&self, index: u16) -> String {
            self.handle.recording_device_name(index)
        }

        fn playout_device_guid(&self, index: u16) -> String {
            self.handle.playout_device_guid(index)
        }

        fn recording_device_guid(&self, index: u16) -> String {
            self.handle.recording_device_guid(index)
        }

        fn set_playout_device(&self, index: u16) -> bool {
            self.handle.set_playout_device(index)
        }

        fn set_recording_device(&self, index: u16) -> bool {
            self.handle.set_recording_device(index)
        }

        fn set_playout_device_by_guid(&self, guid: &str) -> bool {
            self.handle.set_playout_device_by_guid(guid)
        }

        fn set_recording_device_by_guid(&self, guid: &str) -> bool {
            self.handle.set_recording_device_by_guid(guid)
        }

        fn stop_recording(&self) -> bool {
            self.handle.stop_recording()
        }

        fn init_recording(&self) -> bool {
            self.handle.init_recording()
        }

        fn start_recording(&self) -> bool {
            self.handle.start_recording()
        }

        fn recording_is_initialized(&self) -> bool {
            self.handle.recording_is_initialized()
        }

        fn stop_playout(&self) -> bool {
            self.handle.stop_playout()
        }

        fn init_playout(&self) -> bool {
            self.handle.init_playout()
        }

        fn start_playout(&self) -> bool {
            self.handle.start_playout()
        }

        fn playout_is_initialized(&self) -> bool {
            self.handle.playout_is_initialized()
        }

        fn builtin_aec_is_available(&self) -> bool {
            self.handle.builtin_aec_is_available()
        }

        fn builtin_agc_is_available(&self) -> bool {
            self.handle.builtin_agc_is_available()
        }

        fn builtin_ns_is_available(&self) -> bool {
            self.handle.builtin_ns_is_available()
        }

        fn enable_builtin_aec(&self, enable: bool) -> bool {
            self.handle.enable_builtin_aec(enable)
        }

        fn enable_builtin_agc(&self, enable: bool) -> bool {
            self.handle.enable_builtin_agc(enable)
        }

        fn enable_builtin_ns(&self, enable: bool) -> bool {
            self.handle.enable_builtin_ns(enable)
        }

        fn set_adm_recording_enabled(&self, enabled: bool) {
            self.handle.set_adm_recording_enabled(enabled)
        }

        fn adm_recording_enabled(&self) -> bool {
            self.handle.adm_recording_enabled()
        }

        fn set_adm_playout_enabled(&self, enabled: bool) {
            self.handle.set_adm_playout_enabled(enabled)
        }

        fn adm_playout_enabled(&self) -> bool {
            self.handle.adm_playout_enabled()
        }

        fn acquire_platform_adm(&self) -> bool {
            self.handle.acquire_platform_adm()
        }

        fn release_platform_adm(&self) {
            self.handle.release_platform_adm()
        }

        fn platform_adm_ref_count(&self) -> i32 {
            self.handle.platform_adm_ref_count()
        }

        fn is_platform_adm_active(&self) -> bool {
            self.handle.is_platform_adm_active()
        }
    }
}
