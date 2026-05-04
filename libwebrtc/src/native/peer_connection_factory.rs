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

use std::sync::Arc;

use cxx::{SharedPtr, UniquePtr};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use webrtc_sys::{peer_connection_factory as sys_pcf, rtc_error as sys_err, webrtc as sys_rtc};

use crate::{
    audio_source::native::NativeAudioSource,
    audio_track::RtcAudioTrack,
    imp::{audio_track as imp_at, peer_connection as imp_pc, video_track as imp_vt},
    peer_connection::PeerConnection,
    peer_connection_factory::RtcConfiguration,
    rtp_parameters::RtpCapabilities,
    video_source::native::NativeVideoSource,
    video_track::RtcVideoTrack,
    MediaType, RtcError,
};

lazy_static! {
    static ref LOG_SINK: Mutex<Option<UniquePtr<sys_rtc::ffi::LogSink>>> = Default::default();
}

#[derive(Clone)]
pub struct PeerConnectionFactory {
    pub(crate) sys_handle: SharedPtr<sys_pcf::ffi::PeerConnectionFactory>,
}

impl Default for PeerConnectionFactory {
    fn default() -> Self {
        let mut log_sink = LOG_SINK.lock();
        if log_sink.is_none() {
            *log_sink = Some(sys_rtc::ffi::new_log_sink(|msg, _| {
                let msg = msg.strip_suffix("\r\n").or(msg.strip_suffix('\n')).unwrap_or(&msg);
                log::debug!(target: "libwebrtc", "{}", msg);
            }));
        }

        Self { sys_handle: sys_pcf::ffi::create_peer_connection_factory() }
    }
}

impl PeerConnectionFactory {
    pub fn create_peer_connection(
        &self,
        config: RtcConfiguration,
    ) -> Result<PeerConnection, RtcError> {
        let observer = Arc::new(imp_pc::PeerObserver::default());
        let res = self.sys_handle.create_peer_connection(
            config.into(),
            Box::new(sys_pcf::PeerConnectionObserverWrapper::new(observer.clone())),
        );

        match res {
            Ok(sys_handle) => Ok(PeerConnection {
                handle: imp_pc::PeerConnection::configure(sys_handle, observer),
            }),
            Err(e) => Err(unsafe { sys_err::ffi::RtcError::from(e.what()).into() }),
        }
    }

    pub fn create_video_track(&self, label: &str, source: NativeVideoSource) -> RtcVideoTrack {
        RtcVideoTrack {
            handle: imp_vt::RtcVideoTrack::new(
                self.sys_handle.create_video_track(label.to_string(), source.handle.sys_handle()),
            ),
        }
    }

    pub fn create_audio_track(&self, label: &str, source: NativeAudioSource) -> RtcAudioTrack {
        RtcAudioTrack {
            handle: imp_at::RtcAudioTrack {
                sys_handle: self
                    .sys_handle
                    .create_audio_track(label.to_string(), source.handle.sys_handle()),
            },
        }
    }

    /// Create an audio track that uses the Platform ADM for capture.
    ///
    /// This requires that `enable_platform_adm()` was called first.
    /// The track will capture audio from the selected recording device.
    pub fn create_device_audio_track(&self, label: &str) -> RtcAudioTrack {
        RtcAudioTrack {
            handle: imp_at::RtcAudioTrack {
                sys_handle: self.sys_handle.create_device_audio_track(label.to_string()),
            },
        }
    }

    pub fn get_rtp_sender_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.sys_handle.rtp_sender_capabilities(media_type.into()).into()
    }

    pub fn get_rtp_receiver_capabilities(&self, media_type: MediaType) -> RtpCapabilities {
        self.sys_handle.rtp_receiver_capabilities(media_type.into()).into()
    }

    // ===== Device Management Methods =====

    /// Get the number of playout (output) devices
    pub fn playout_devices(&self) -> i16 {
        self.sys_handle.playout_devices()
    }

    /// Get the number of recording (input) devices
    pub fn recording_devices(&self) -> i16 {
        self.sys_handle.recording_devices()
    }

    /// Get the name of a playout device by index
    pub fn playout_device_name(&self, index: u16) -> String {
        self.sys_handle.playout_device_name(index)
    }

    /// Get the name of a recording device by index
    pub fn recording_device_name(&self, index: u16) -> String {
        self.sys_handle.recording_device_name(index)
    }

    /// Set the playout device by index
    /// Returns 0 on success, negative on error.
    pub fn set_playout_device(&self, index: u16) -> i32 {
        self.sys_handle.set_playout_device(index)
    }

    /// Set the recording device by index
    /// Returns 0 on success, negative on error.
    pub fn set_recording_device(&self, index: u16) -> i32 {
        self.sys_handle.set_recording_device(index)
    }

    /// Stop recording (clears initialized state, allowing device switch)
    pub fn stop_recording(&self) -> i32 {
        self.sys_handle.stop_recording()
    }

    /// Initialize recording
    pub fn init_recording(&self) -> i32 {
        self.sys_handle.init_recording()
    }

    /// Start recording
    pub fn start_recording(&self) -> i32 {
        self.sys_handle.start_recording()
    }

    /// Check if recording is initialized
    pub fn recording_is_initialized(&self) -> bool {
        self.sys_handle.recording_is_initialized()
    }

    /// Stop playout (clears initialized state, allowing device switch)
    pub fn stop_playout(&self) -> i32 {
        self.sys_handle.stop_playout()
    }

    /// Initialize playout
    pub fn init_playout(&self) -> i32 {
        self.sys_handle.init_playout()
    }

    /// Start playout
    pub fn start_playout(&self) -> i32 {
        self.sys_handle.start_playout()
    }

    /// Check if playout is initialized
    pub fn playout_is_initialized(&self) -> bool {
        self.sys_handle.playout_is_initialized()
    }

    // ===== Built-in Audio Processing Methods =====
    // These control hardware AEC/AGC/NS on platforms that support it (iOS, some Android)

    /// Check if built-in (hardware) AEC is available on this device.
    ///
    /// Returns true on iOS (VPIO) and some Android devices.
    /// Returns false on desktop platforms (macOS, Windows, Linux).
    pub fn builtin_aec_is_available(&self) -> bool {
        self.sys_handle.builtin_aec_is_available()
    }

    /// Check if built-in (hardware) AGC is available on this device.
    ///
    /// Returns true on iOS (VPIO) and some Android devices.
    /// Returns false on desktop platforms (macOS, Windows, Linux).
    pub fn builtin_agc_is_available(&self) -> bool {
        self.sys_handle.builtin_agc_is_available()
    }

    /// Check if built-in (hardware) NS is available on this device.
    ///
    /// Returns true on iOS (VPIO) and some Android devices.
    /// Returns false on desktop platforms (macOS, Windows, Linux).
    pub fn builtin_ns_is_available(&self) -> bool {
        self.sys_handle.builtin_ns_is_available()
    }

    /// Enable or disable built-in (hardware) AEC.
    ///
    /// When disabled on platforms that support it, WebRTC's software AEC
    /// will be used instead.
    ///
    /// Returns 0 on success, negative on error.
    pub fn enable_builtin_aec(&self, enable: bool) -> i32 {
        self.sys_handle.enable_builtin_aec(enable)
    }

    /// Enable or disable built-in (hardware) AGC.
    ///
    /// When disabled on platforms that support it, WebRTC's software AGC
    /// will be used instead.
    ///
    /// Returns 0 on success, negative on error.
    pub fn enable_builtin_agc(&self, enable: bool) -> i32 {
        self.sys_handle.enable_builtin_agc(enable)
    }

    /// Enable or disable built-in (hardware) NS.
    ///
    /// When disabled on platforms that support it, WebRTC's software NS
    /// will be used instead.
    ///
    /// Returns 0 on success, negative on error.
    pub fn enable_builtin_ns(&self, enable: bool) -> i32 {
        self.sys_handle.enable_builtin_ns(enable)
    }

    /// Control whether ADM recording (microphone) is enabled.
    ///
    /// When disabled, WebRTC's calls to InitRecording/StartRecording will be no-ops.
    /// Use this when only using NativeAudioSource (no microphone capture needed).
    /// This prevents the microphone from interfering with the audio pipeline.
    pub fn set_adm_recording_enabled(&self, enabled: bool) {
        self.sys_handle.set_adm_recording_enabled(enabled)
    }

    /// Check if ADM recording (microphone) is enabled.
    pub fn adm_recording_enabled(&self) -> bool {
        self.sys_handle.adm_recording_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_peer_connection_factory() {
        let _ = env_logger::builder().is_test(true).try_init();

        let factory = PeerConnectionFactory::default();
        let source = NativeVideoSource::default();
        let _track = factory.create_video_track("test", source);
        drop(factory);
    }
}
