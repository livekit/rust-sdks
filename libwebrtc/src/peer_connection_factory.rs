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
        audio_source::native::NativeAudioSource,
        audio_track::RtcAudioTrack,
        imp::peer_connection_factory::AdmDelegateType,
        video_source::native::NativeVideoSource,
        video_track::RtcVideoTrack,
    };

    pub trait PeerConnectionFactoryExt {
        fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack;
        fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack;

        /// Create an audio track that uses the Platform ADM for capture.
        ///
        /// This requires that `enable_platform_adm()` was called first.
        /// The track will capture audio from the selected recording device.
        fn create_device_audio_track(&self, label: &str) -> RtcAudioTrack;

        // ADM Management
        /// Enable platform ADM (WebRTC's built-in device management)
        /// Returns true if successful
        fn enable_platform_adm(&self) -> bool;

        /// Clear ADM delegate, reverting to stub behavior (NativeAudioSource mode)
        fn clear_adm_delegate(&self);

        /// Get the current ADM delegate type
        fn adm_delegate_type(&self) -> AdmDelegateType;

        /// Check if an ADM delegate is active
        fn has_adm_delegate(&self) -> bool;

        // Device enumeration (only works with platform/custom ADM)
        fn playout_devices(&self) -> i16;
        fn recording_devices(&self) -> i16;
        fn playout_device_name(&self, index: u16) -> String;
        fn recording_device_name(&self, index: u16) -> String;

        // Device selection (only works with platform/custom ADM)
        fn set_playout_device(&self, index: u16) -> i32;
        fn set_recording_device(&self, index: u16) -> i32;

        // Recording control (for device switching while active)
        fn stop_recording(&self) -> i32;
        fn init_recording(&self) -> i32;
        fn start_recording(&self) -> i32;
        fn recording_is_initialized(&self) -> bool;

        // Playout control (for device switching while active)
        fn stop_playout(&self) -> i32;
        fn init_playout(&self) -> i32;
        fn start_playout(&self) -> i32;
        fn playout_is_initialized(&self) -> bool;
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

        fn enable_platform_adm(&self) -> bool {
            self.handle.enable_platform_adm()
        }

        fn clear_adm_delegate(&self) {
            self.handle.clear_adm_delegate();
        }

        fn adm_delegate_type(&self) -> AdmDelegateType {
            self.handle.adm_delegate_type()
        }

        fn has_adm_delegate(&self) -> bool {
            self.handle.has_adm_delegate()
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

        fn set_playout_device(&self, index: u16) -> i32 {
            self.handle.set_playout_device(index)
        }

        fn set_recording_device(&self, index: u16) -> i32 {
            self.handle.set_recording_device(index)
        }

        fn stop_recording(&self) -> i32 {
            self.handle.stop_recording()
        }

        fn init_recording(&self) -> i32 {
            self.handle.init_recording()
        }

        fn start_recording(&self) -> i32 {
            self.handle.start_recording()
        }

        fn recording_is_initialized(&self) -> bool {
            self.handle.recording_is_initialized()
        }

        fn stop_playout(&self) -> i32 {
            self.handle.stop_playout()
        }

        fn init_playout(&self) -> i32 {
            self.handle.init_playout()
        }

        fn start_playout(&self) -> i32 {
            self.handle.start_playout()
        }

        fn playout_is_initialized(&self) -> bool {
            self.handle.playout_is_initialized()
        }
    }
}
