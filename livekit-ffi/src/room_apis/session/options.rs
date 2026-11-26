// Copyright 2026 LiveKit, Inc.
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

//! Core publishing values: metadata only, no parallel options model or conversion.
use super::TrackSource;
pub use livekit::options::{
    AudioEncoding, AudioPreset, DegradationPreference, FrameMetadataFeatures, TrackPublishOptions,
    VideoCodec, VideoEncoderBackend, VideoEncoding, VideoPreset,
};

/// Core AudioEncoding.
#[uniffi::remote(Record)]
pub struct AudioEncoding {
    pub max_bitrate: u64,
}

/// Core AudioPreset.
#[uniffi::remote(Record)]
pub struct AudioPreset {
    pub encoding: AudioEncoding,
}

/// Core VideoEncoding.
#[uniffi::remote(Record)]
pub struct VideoEncoding {
    pub max_bitrate: u64,
    pub max_framerate: f64,
}

/// Core VideoPreset.
#[uniffi::remote(Record)]
pub struct VideoPreset {
    pub encoding: VideoEncoding,
    pub width: u32,
    pub height: u32,
}

/// Core VideoCodec.
#[uniffi::remote(Enum)]
pub enum VideoCodec {
    VP8,
    H264,
    VP9,
    AV1,
    H265,
}

/// Core DegradationPreference.
#[uniffi::remote(Enum)]
pub enum DegradationPreference {
    MaintainFramerate,
    MaintainResolution,
    Balanced,
    MaintainFramerateAndResolution,
}

/// All core publishing options. Start with `default_track_publish_options` for core defaults.
#[uniffi::remote(Record)]
pub struct TrackPublishOptions {
    pub video_encoding: Option<VideoEncoding>,
    pub audio_encoding: Option<AudioEncoding>,
    pub video_codec: VideoCodec,
    pub dtx: bool,
    pub red: bool,
    pub simulcast: bool,
    pub simulcast_layers: Option<Vec<VideoPreset>>,
    pub source: TrackSource,
    pub stream: String,
    pub preconnect_buffer: bool,
    pub frame_metadata_features: FrameMetadataFeatures,
    pub video_encoder: VideoEncoderBackend,
    pub scalability_mode: Option<String>,
    pub degradation_preference: Option<DegradationPreference>,
}

/// Returns the authoritative core defaults, including nested options.
#[uniffi::export]
pub fn default_track_publish_options() -> TrackPublishOptions {
    TrackPublishOptions::default()
}

use super::{
    encryption::NativeEncryptionOptions, media_frames::FfiTrack, ContinualGatheringPolicy,
    IceServer, IceTransportPolicy, NativeSessionError,
};
use livekit::{prelude::*, webrtc::peer_connection_factory::RtcConfiguration};
use std::{sync::Arc, time::Duration};

/// Immutable builder over the actual core RoomOptions, including its nested defaults.
/// Core's non-exhaustive options and private stream limit cannot be remote records.
#[derive(Clone, Debug, uniffi::Object)]
pub struct FfiRoomOptions(pub(super) RoomOptions);

#[uniffi::export]
impl FfiRoomOptions {
    /// Uses the current core defaults without duplicating them in binding metadata.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self(RoomOptions::default())
    }
    /// Reads core's auto subscribe.
    pub fn auto_subscribe(&self) -> bool {
        self.0.auto_subscribe
    }
    /// Returns options with the requested auto subscribe.
    pub fn with_auto_subscribe(&self, value: bool) -> Self {
        let mut options = self.clone();
        options.0.auto_subscribe = value;
        options
    }
    /// Reads core's adaptive stream.
    pub fn adaptive_stream(&self) -> bool {
        self.0.adaptive_stream
    }
    /// Returns options with the requested adaptive stream.
    pub fn with_adaptive_stream(&self, value: bool) -> Self {
        let mut options = self.clone();
        options.0.adaptive_stream = value;
        options
    }
    /// Reads core's dynacast.
    pub fn dynacast(&self) -> bool {
        self.0.dynacast
    }
    /// Returns options with the requested dynacast.
    pub fn with_dynacast(&self, value: bool) -> Self {
        let mut options = self.clone();
        options.0.dynacast = value;
        options
    }
    /// Reads core's single peer connection.
    pub fn single_peer_connection(&self) -> bool {
        self.0.single_peer_connection
    }
    /// Returns options with the requested single peer connection.
    pub fn with_single_peer_connection(&self, value: bool) -> Self {
        let mut options = self.clone();
        options.0.single_peer_connection = value;
        options
    }
    /// Reads core's join retries.
    pub fn join_retries(&self) -> u32 {
        self.0.join_retries
    }
    /// Returns options with the requested join retries.
    pub fn with_join_retries(&self, value: u32) -> Self {
        let mut options = self.clone();
        options.0.join_retries = value;
        options
    }
    /// Reads core's connect timeout.
    pub fn connect_timeout(&self) -> Duration {
        self.0.connect_timeout
    }
    /// Returns options with the requested connect timeout.
    pub fn with_connect_timeout(&self, value: Duration) -> Self {
        let mut options = self.clone();
        options.0.connect_timeout = value;
        options
    }
    /// Configures every core RTC option, including SCTP SNAP.
    pub fn with_rtc_config(
        &self,
        ice_servers: Vec<IceServer>,
        ice_transport_policy: IceTransportPolicy,
        continual_gathering_policy: ContinualGatheringPolicy,
        enable_sctp_snap: bool,
    ) -> Self {
        let mut options = self.clone();
        let mut rtc = RtcConfiguration::default();
        rtc.ice_servers = ice_servers;
        rtc.ice_transport_type = ice_transport_policy;
        rtc.continual_gathering_policy = continual_gathering_policy;
        rtc.enable_sctp_snap = enable_sctp_snap;
        options.0.rtc_config = rtc;
        options
    }
    /// Returns the configured ICE servers.
    pub fn ice_servers(&self) -> Vec<IceServer> {
        self.0.rtc_config.ice_servers.clone()
    }
    /// Returns core's ICE policy.
    pub fn ice_transport_policy(&self) -> IceTransportPolicy {
        self.0.rtc_config.ice_transport_type
    }
    /// Returns core's gathering policy.
    pub fn continual_gathering_policy(&self) -> ContinualGatheringPolicy {
        self.0.rtc_config.continual_gathering_policy
    }
    /// Returns whether SCTP SNAP is enabled.
    pub fn enable_sctp_snap(&self) -> bool {
        self.0.rtc_config.enable_sctp_snap
    }
    /// Configures the SDK and optional layered SDK versions reported to the server.
    pub fn with_sdk(&self, sdk: String, sdk_version: String, other_sdks: Option<String>) -> Self {
        let mut options = self.clone();
        options.0.sdk_options.sdk = sdk;
        options.0.sdk_options.sdk_version = sdk_version;
        options.0.sdk_options.other_sdks = other_sdks;
        options
    }
    /// Returns the configured SDK name.
    pub fn sdk(&self) -> String {
        self.0.sdk_options.sdk.clone()
    }
    /// Returns the configured SDK version.
    pub fn sdk_version(&self) -> String {
        self.0.sdk_options.sdk_version.clone()
    }
    /// Returns the configured layered SDK versions.
    pub fn other_sdks(&self) -> Option<String> {
        self.0.sdk_options.other_sdks.clone()
    }
    /// Sets the core stream limit; None restores its default. Checked for the target pointer width.
    pub fn with_max_stream_payload_bytes(
        &self,
        value: Option<u64>,
    ) -> Result<Self, NativeSessionError> {
        let mut options = self.clone();
        options.0.data_stream = match value {
            None => Default::default(),
            Some(value) => livekit::RoomDataStreamOptions::default().with_max_payload_byte_length(
                usize::try_from(value).map_err(|_| NativeSessionError::InvalidState {
                    reason: "stream limit exceeds target address space".into(),
                })?,
            ),
        };
        Ok(options)
    }
    /// Sets core encryption or legacy media-only encryption; None restores plaintext.
    pub fn with_encryption(&self, value: Option<NativeEncryptionOptions>) -> Self {
        let mut options = self.clone();
        #[allow(deprecated)]
        {
            options.0.e2ee = None;
            options.0.encryption = None;
            if let Some(value) = value {
                if value.legacy_media_only {
                    options.0.e2ee = Some(value.core());
                } else {
                    options.0.encryption = Some(value.core());
                }
            }
        }
        options
    }
    /// Adds a local track to core's publish-on-connect list. Remote tracks are rejected.
    pub fn with_publish_track(
        &self,
        track: Arc<FfiTrack>,
        publish_options: TrackPublishOptions,
    ) -> Result<Self, NativeSessionError> {
        let track = match &track.0 {
            Track::LocalAudio(track) => LocalTrack::Audio(track.clone()),
            Track::LocalVideo(track) => LocalTrack::Video(track.clone()),
            _ => {
                return Err(NativeSessionError::InvalidState {
                    reason: "only local tracks can be published while connecting".into(),
                })
            }
        };
        let mut options = self.clone();
        options.0.publish_tracks.push((track, publish_options));
        Ok(options)
    }
    /// Returns the core publishing options for each configured initial track.
    pub fn publish_track_options(&self) -> Vec<TrackPublishOptions> {
        self.0.publish_tracks.iter().map(|(_, options)| options.clone()).collect()
    }
    /// Returns the number of tracks configured for publishing while connecting.
    pub fn publish_track_count(&self) -> u64 {
        self.0.publish_tracks.len() as u64
    }
}

impl Default for FfiRoomOptions {
    fn default() -> Self {
        Self::new()
    }
}
