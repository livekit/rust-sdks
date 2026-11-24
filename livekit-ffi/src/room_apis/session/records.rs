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

//! Value records, enums and the error type of the typed room surface.
//!
//! Every exported name carries the `Native` prefix so that the generated
//! bindings never collide with the public types of a host SDK.

use super::encryption::NativeEncryptionOptions;
use super::errors::{NativeRequestFailureCode, NativeRtcFailureKind, NativeSignalFailure};
pub use livekit::webrtc::native::frame_cryptor::EncryptionState as NativeEncryptionState;
use std::{collections::HashMap, time::Duration};

/// Native media cryptor state, forwarded from the authoritative room.
#[uniffi::remote(Enum)]
pub enum NativeEncryptionState {
    New,
    Ok,
    EncryptionFailed,
    DecryptionFailed,
    MissingKey,
    KeyRatcheted,
    InternalError,
}

// The core enums are exported as they are. `uniffi::remote` emits only the
// converters, and its exhaustive match fails the build when core adds a variant.
pub use livekit::{
    e2ee::EncryptionType as NativeEncryptionType,
    participant::{
        ConnectionQuality as NativeConnectionQuality, DisconnectReason as NativeDisconnectReason,
        ParticipantKind as NativeParticipantKind,
        ParticipantKindDetail as NativeParticipantKindDetail,
        ParticipantState as NativeParticipantState,
    },
    publication::{
        PermissionStatus as NativePermissionStatus, SubscriptionStatus as NativeSubscriptionStatus,
    },
    track::{
        StreamState as NativeStreamState, TrackKind as NativeTrackKind,
        TrackSource as NativeTrackSource,
    },
    DataPacketKind as NativeDataPacketKind,
};
use livekit::{prelude::*, RoomInfo};
use livekit_protocol as lk_proto;

#[uniffi::remote(Enum)]
pub enum NativeConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

#[uniffi::remote(Enum)]
pub enum NativeDisconnectReason {
    UnknownReason,
    ClientInitiated,
    DuplicateIdentity,
    ServerShutdown,
    ParticipantRemoved,
    RoomDeleted,
    StateMismatch,
    JoinFailure,
    Migration,
    SignalClose,
    RoomClosed,
    UserUnavailable,
    UserRejected,
    SipTrunkFailure,
    ConnectionTimeout,
    MediaFailure,
    AgentError,
}

#[uniffi::remote(Enum)]
pub enum NativeParticipantKind {
    Standard,
    Ingress,
    Egress,
    Sip,
    Agent,
    Connector,
    Bridge,
}

#[uniffi::remote(Enum)]
pub enum NativeParticipantKindDetail {
    CloudAgent,
    Forwarded,
    ConnectorWhatsapp,
    ConnectorTwilio,
    BridgeRtsp,
    Simulation,
}

#[uniffi::remote(Enum)]
pub enum NativeParticipantState {
    Joining,
    Joined,
    Active,
    Disconnected,
}

#[uniffi::remote(Enum)]
pub enum NativeEncryptionType {
    None,
    Gcm,
    Custom,
}

#[uniffi::remote(Enum)]
pub enum NativeSubscriptionStatus {
    Desired,
    Subscribed,
    Unsubscribed,
}

#[uniffi::remote(Enum)]
pub enum NativePermissionStatus {
    Allowed,
    NotAllowed,
}

#[uniffi::remote(Enum)]
pub enum NativeTrackKind {
    Audio,
    Video,
}

#[uniffi::remote(Enum)]
pub enum NativeTrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

#[uniffi::remote(Enum)]
pub enum NativeStreamState {
    Active,
    Paused,
}

#[uniffi::remote(Enum)]
pub enum NativeDataPacketKind {
    Lossy,
    Reliable,
}

/// The lifecycle of a [`NativeRoomSession`](super::NativeRoomSession).
///
/// `Disconnected` is final. A new connection needs a new session.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeConnectionState {
    /// The session exists and `connect` was not called.
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
}

/// The error type of the typed room surface.
#[derive(uniffi::Error, thiserror::Error, Clone, Debug, PartialEq)]
pub enum NativeSessionError {
    /// `cancel` ended the session or the cursor.
    #[error("cancelled")]
    Cancelled,
    /// The session ended. The cause is a disconnect or a failed connection.
    #[error("closed")]
    Closed,
    /// The call is not legal in the current lifecycle state.
    #[error("invalid state: {reason}")]
    InvalidState { reason: String },
    /// The connection to the server failed.
    #[error("connection failed: {reason}")]
    Connection { reason: String },
    /// The server refused a request.
    #[error("request refused: {reason}")]
    Request { reason: String },
    /// Exact signaling cause when the public core retains it. Older string cases remain for compatibility.
    #[error("signal failure: {failure:?}")]
    Signal { failure: NativeSignalFailure },
    #[error("request refused: {code:?}: {reason}")]
    RequestFailure { code: NativeRequestFailureCode, reason: String },
    #[error("RTC failure: {kind:?}: {reason}")]
    Rtc { kind: NativeRtcFailureKind, reason: String },
    #[error("track is already published")]
    TrackAlreadyPublished,
    /// The consumer did not keep up and the event backlog passed its limit.
    #[error("event backlog overflow at {items} items and {bytes} bytes")]
    Overflow { items: u32, bytes: u64 },
    /// The Rust core does not have this capability.
    #[error("unsupported: {reason}")]
    Unsupported { reason: String },
    #[error("internal failure: {reason}")]
    Internal { reason: String },
}

/// Returns `true` when `error` means that the session or cursor ended.
///
/// The classification stays in Rust so that every binding agrees on it.
#[uniffi::export]
pub fn native_session_error_is_shutdown(error: NativeSessionError) -> bool {
    matches!(error, NativeSessionError::Cancelled | NativeSessionError::Closed)
}

/// ICE server credentials forwarded directly to core RTC configuration.
#[derive(uniffi::Record, Clone)]
pub struct NativeIceServer {
    pub urls: Vec<String>,
    #[uniffi(default = "")]
    pub username: String,
    #[uniffi(default = "")]
    pub password: String,
}

impl std::fmt::Debug for NativeIceServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // URLs can contain user-info credentials too.
        f.debug_struct("NativeIceServer")
            .field("url_count", &self.urls.len())
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

pub use livekit::webrtc::peer_connection_factory::{
    ContinualGatheringPolicy as NativeContinualGatheringPolicy,
    IceTransportsType as NativeIceTransportPolicy,
};
/// Core ICE candidate policy.
#[uniffi::remote(Enum)]
pub enum NativeIceTransportPolicy {
    Relay,
    NoHost,
    All,
}
/// Core ICE gathering policy.
#[uniffi::remote(Enum)]
pub enum NativeContinualGatheringPolicy {
    GatherOnce,
    GatherContinually,
}

/// Options for [`NativeRoomSession::connect`](super::NativeRoomSession::connect).
///
/// Every field has a default, so a new field does not break existing callers.
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeConnectOptions {
    #[uniffi(default = true)]
    pub auto_subscribe: bool,
    #[uniffi(default = false)]
    pub adaptive_stream: bool,
    #[uniffi(default = false)]
    pub dynacast: bool,
    #[uniffi(default = true)]
    pub single_peer_connection: bool,
    #[uniffi(default = 3)]
    pub join_retries: u32,
    /// Timeout for each signal connection attempt. `None` keeps the core default.
    #[uniffi(default = None)]
    pub connect_timeout_ms: Option<u64>,
    /// Name of the host SDK, reported to the server. `None` reports `rust`.
    #[uniffi(default = None)]
    pub sdk: Option<String>,
    #[uniffi(default = None)]
    pub sdk_version: Option<String>,
    /// Maximum incoming stream payload bytes; None keeps the core default.
    #[uniffi(default = None)]
    pub max_stream_payload_bytes: Option<u32>,
    /// AES-GCM provider/configuration; None preserves plaintext behavior.
    #[uniffi(default = None)]
    pub encryption: Option<NativeEncryptionOptions>,
    /// Empty preserves core's server-provided ICE list; nonempty overrides that list.
    #[uniffi(default = [])]
    pub ice_servers: Vec<NativeIceServer>,
    #[uniffi(default = None)]
    pub ice_transport_policy: Option<NativeIceTransportPolicy>,
    #[uniffi(default = None)]
    pub continual_gathering_policy: Option<NativeContinualGatheringPolicy>,
    /// Only the core's advertised default can be requested: RoomOptions has no override.
    #[uniffi(default = None)]
    pub client_protocol: Option<i32>,
}

impl NativeConnectOptions {
    pub(super) fn validate(&self) -> Result<(), NativeSessionError> {
        if self
            .client_protocol
            .is_some_and(|p| p != livekit_signaling::SignalOptions::default().client_protocol)
        {
            return Err(NativeSessionError::Unsupported { reason: "RoomOptions does not expose a clientProtocol override; only the core default is supported".into() });
        }
        Ok(())
    }
}

impl From<NativeConnectOptions> for RoomOptions {
    fn from(options: NativeConnectOptions) -> Self {
        let mut core = RoomOptions::default();
        if let Some(encryption) = options.encryption {
            if encryption.legacy_media_only {
                #[allow(deprecated)]
                {
                    core.e2ee = Some(encryption.core());
                }
            } else {
                core.encryption = Some(encryption.core());
            }
        }
        core.rtc_config.ice_servers = options
            .ice_servers
            .into_iter()
            .map(|server| livekit::webrtc::peer_connection_factory::IceServer {
                urls: server.urls,
                username: server.username,
                password: server.password,
            })
            .collect();
        if let Some(policy) = options.ice_transport_policy {
            core.rtc_config.ice_transport_type = policy;
        }
        if let Some(policy) = options.continual_gathering_policy {
            core.rtc_config.continual_gathering_policy = policy;
        }
        core.auto_subscribe = options.auto_subscribe;
        core.adaptive_stream = options.adaptive_stream;
        core.dynacast = options.dynacast;
        core.single_peer_connection = options.single_peer_connection;
        core.join_retries = options.join_retries;
        if let Some(ms) = options.connect_timeout_ms {
            core.connect_timeout = Duration::from_millis(ms);
        }
        if let Some(limit) = options.max_stream_payload_bytes {
            core.data_stream = core.data_stream.with_max_payload_byte_length(limit as usize);
        }
        if let Some(sdk) = options.sdk {
            core.sdk_options.sdk = sdk;
        }
        if let Some(version) = options.sdk_version {
            core.sdk_options.sdk_version = version;
        }
        core
    }
}

/// Options for [`NativeRoomSession::events`](super::NativeRoomSession::events).
#[derive(uniffi::Record, Clone, Copy, Debug)]
pub struct NativeEventCursorOptions {
    /// Largest number of events that wait for the consumer.
    #[uniffi(default = 4096)]
    pub max_items: u32,
    /// Largest sum of payload bytes that waits for the consumer.
    #[uniffi(default = 16777216)]
    pub max_bytes: u64,
}

/// Options for [`NativeRoomSession::publish_data`](super::NativeRoomSession::publish_data).
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeDataPublishOptions {
    #[uniffi(default = true)]
    pub reliable: bool,
    #[uniffi(default = None)]
    pub topic: Option<String>,
    /// Empty means every participant.
    #[uniffi(default = [])]
    pub destination_identities: Vec<String>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeRoomInfo {
    /// `None` until the server assigns the identifier.
    pub sid: Option<String>,
    pub name: String,
    pub metadata: String,
    pub num_participants: u32,
    pub num_publishers: u32,
    pub max_participants: u32,
    pub active_recording: bool,
    pub empty_timeout: u32,
    pub departure_timeout: u32,
    /// Milliseconds since the Unix epoch.
    pub creation_time: i64,
}

impl From<&RoomInfo> for NativeRoomInfo {
    fn from(info: &RoomInfo) -> Self {
        Self {
            sid: info.sid.clone().map(Into::into),
            name: info.name.clone(),
            metadata: info.metadata.clone(),
            num_participants: info.num_participants,
            num_publishers: info.num_publishers,
            max_participants: info.max_participants,
            active_recording: info.active_recording,
            empty_timeout: info.empty_timeout,
            departure_timeout: info.departure_timeout,
            creation_time: info.creation_time,
        }
    }
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeParticipantPermission {
    pub can_subscribe: bool,
    pub can_publish: bool,
    pub can_publish_data: bool,
    /// Empty means every source.
    pub can_publish_sources: Vec<NativeTrackSource>,
    pub hidden: bool,
    pub can_update_metadata: bool,
    pub can_subscribe_metrics: bool,
    pub can_manage_agent_session: bool,
}

impl From<lk_proto::ParticipantPermission> for NativeParticipantPermission {
    fn from(p: lk_proto::ParticipantPermission) -> Self {
        let can_publish_sources = p
            .can_publish_sources
            .iter()
            .filter_map(|&source| lk_proto::TrackSource::try_from(source).ok())
            .map(|source| match source {
                lk_proto::TrackSource::Unknown => NativeTrackSource::Unknown,
                lk_proto::TrackSource::Camera => NativeTrackSource::Camera,
                lk_proto::TrackSource::Microphone => NativeTrackSource::Microphone,
                lk_proto::TrackSource::ScreenShare => NativeTrackSource::Screenshare,
                lk_proto::TrackSource::ScreenShareAudio => NativeTrackSource::ScreenshareAudio,
            })
            .collect();
        Self {
            can_subscribe: p.can_subscribe,
            can_publish: p.can_publish,
            can_publish_data: p.can_publish_data,
            can_publish_sources,
            hidden: p.hidden,
            can_update_metadata: p.can_update_metadata,
            can_subscribe_metrics: p.can_subscribe_metrics,
            can_manage_agent_session: p.can_manage_agent_session,
        }
    }
}

/// A participant. `identity` is the stable key within one session.
///
/// Publications are separate records. See [`NativeTrackPublication`].
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeParticipant {
    pub identity: String,
    pub sid: String,
    pub is_local: bool,
    pub name: String,
    pub state: NativeParticipantState,
    pub kind: NativeParticipantKind,
    pub kind_details: Vec<NativeParticipantKindDetail>,
    pub metadata: String,
    pub attributes: HashMap<String, String>,
    pub is_speaking: bool,
    pub audio_level: f32,
    pub connection_quality: NativeConnectionQuality,
    /// Milliseconds since the Unix epoch.
    pub joined_at: i64,
    pub permission: Option<NativeParticipantPermission>,
    pub is_encrypted: bool,
    pub disconnect_reason: NativeDisconnectReason,
    pub client_protocol: i32,
    /// Advertised remote capabilities. None for local participants: core has no accessor.
    pub capabilities: Option<Vec<crate::data_stream::common::ClientCapability>>,
}

impl From<&Participant> for NativeParticipant {
    fn from(p: &Participant) -> Self {
        Self {
            identity: p.identity().into(),
            sid: p.sid().into(),
            is_local: matches!(p, Participant::Local(_)),
            name: p.name(),
            state: p.state(),
            kind: p.kind(),
            kind_details: p.kind_details(),
            metadata: p.metadata(),
            attributes: p.attributes(),
            is_speaking: p.is_speaking(),
            audio_level: p.audio_level(),
            connection_quality: p.connection_quality(),
            joined_at: p.joined_at(),
            permission: p.permission().map(Into::into),
            is_encrypted: p.is_encrypted(),
            disconnect_reason: p.disconnect_reason(),
            client_protocol: p.client_protocol(),
            capabilities: match p {
                Participant::Remote(remote) => {
                    Some(remote.capabilities().into_iter().map(Into::into).collect())
                }
                Participant::Local(_) => None,
            },
        }
    }
}

/// The part of a publication that only a remote track has.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeRemotePublicationState {
    pub subscription_status: NativeSubscriptionStatus,
    pub permission_status: NativePermissionStatus,
    /// `false` when the receiver paused the track.
    pub is_enabled: bool,
    /// `None` when the track is not subscribed.
    pub stream_state: Option<NativeStreamState>,
}

/// A track publication. `sid` is the key. A local track gets a new `sid` when
/// the core publishes it again after a full reconnect.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeTrackPublication {
    pub sid: String,
    pub participant_identity: String,
    pub name: String,
    pub kind: NativeTrackKind,
    pub source: NativeTrackSource,
    pub is_muted: bool,
    pub simulcasted: bool,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub encryption_type: NativeEncryptionType,
    /// `None` for a local publication.
    pub remote: Option<NativeRemotePublicationState>,
}

impl NativeTrackPublication {
    pub(super) fn new(participant_identity: &str, publication: &TrackPublication) -> Self {
        let remote = match publication {
            TrackPublication::Local(_) => None,
            TrackPublication::Remote(p) => Some(NativeRemotePublicationState {
                subscription_status: p.subscription_status(),
                permission_status: p.permission_status(),
                is_enabled: p.is_enabled(),
                stream_state: p.track().map(|track| track.stream_state()),
            }),
        };
        let dimension = publication.dimension();
        Self {
            sid: publication.sid().into(),
            participant_identity: participant_identity.to_owned(),
            name: publication.name(),
            kind: publication.kind(),
            source: publication.source(),
            is_muted: publication.is_muted(),
            simulcasted: publication.simulcasted(),
            width: dimension.0,
            height: dimension.1,
            mime_type: publication.mime_type(),
            encryption_type: publication.encryption_type(),
            remote,
        }
    }
}

/// A snapshot of the session. The newest snapshot replaces every older one.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeRoomState {
    /// Remote data track publications known at this revision.
    pub data_tracks: Vec<NativeDataTrackPublication>,
    /// Unique for each session in this process. Entity keys are only
    /// comparable within one generation.
    pub generation: u64,
    /// Grows by at least one for each change to this snapshot.
    pub revision: u64,
    pub connection: NativeConnectionState,
    /// `None` until the session is `Disconnected`.
    pub disconnect_reason: Option<NativeDisconnectReason>,
    /// `None` before the first connection.
    pub room: Option<NativeRoomInfo>,
    pub local_participant: Option<NativeParticipant>,
    /// In no particular order.
    pub remote_participants: Vec<NativeParticipant>,
    /// Local and remote publications, in no particular order.
    pub publications: Vec<NativeTrackPublication>,
}

/// One active speaker.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeSpeaker {
    pub identity: String,
    pub audio_level: f32,
}

/// A data packet from a remote participant.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeDataPacket {
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub kind: NativeDataPacketKind,
    /// `None` when the server sent the packet.
    pub participant_identity: Option<String>,
}

/// An ordered room event. Each record is the value after the change.
///
/// New variants can appear in a later version, so a consumer must ignore a
/// variant that it does not know.
#[derive(uniffi::Enum, Clone, PartialEq)]
pub enum NativeRoomEvent {
    /// An incoming RPC awaiting a reply before its deadline.
    RpcRequested {
        invocation: std::sync::Arc<super::NativeRpcInvocation>,
    },
    /// A byte reader owned by the connected core room.
    ByteStreamOpened {
        reader: std::sync::Arc<crate::data_stream::incoming::ByteStreamReader>,
        identity: String,
    },
    /// A text reader owned by the connected core room.
    TextStreamOpened {
        reader: std::sync::Arc<crate::data_stream::incoming::TextStreamReader>,
        identity: String,
    },
    /// A remote data track became available.
    DataTrackPublished {
        track: std::sync::Arc<crate::data_track::remote::RemoteDataTrack>,
        identity: String,
    },
    /// A remote data track was unpublished.
    DataTrackUnpublished {
        track: std::sync::Arc<crate::data_track::remote::RemoteDataTrack>,
        identity: String,
        sid: String,
    },

    ConnectionStateChanged {
        state: NativeConnectionState,
    },
    /// The last event of a session.
    Disconnected {
        reason: NativeDisconnectReason,
    },
    /// Counters or recording state changed. Only the newest value is kept.
    RoomUpdated {
        room: NativeRoomInfo,
    },
    RoomMetadataChanged {
        room: NativeRoomInfo,
        old_metadata: String,
    },
    RoomMoved {
        room: NativeRoomInfo,
    },
    TokenRefreshed {
        token: String,
    },
    ParticipantConnected {
        participant: NativeParticipant,
    },
    ParticipantDisconnected {
        participant: NativeParticipant,
    },
    /// A change that has no specific event. Only the newest value is kept.
    ParticipantUpdated {
        participant: NativeParticipant,
    },
    ParticipantMetadataChanged {
        participant: NativeParticipant,
        old_metadata: String,
    },
    ParticipantNameChanged {
        participant: NativeParticipant,
        old_name: String,
    },
    ParticipantAttributesChanged {
        participant: NativeParticipant,
        changed_attributes: HashMap<String, String>,
    },
    ParticipantPermissionChanged {
        participant: NativeParticipant,
    },
    ParticipantEncryptionStatusChanged {
        participant: NativeParticipant,
    },
    EncryptionStateChanged {
        participant: NativeParticipant,
        state: NativeEncryptionState,
    },
    /// Only the newest value for each participant is kept.
    ConnectionQualityChanged {
        participant: NativeParticipant,
    },
    /// A participant that is absent from `speakers` stopped speaking. Only
    /// the newest value is kept.
    ActiveSpeakersChanged {
        speakers: Vec<NativeSpeaker>,
    },
    TrackPublished {
        publication: NativeTrackPublication,
    },
    TrackUnpublished {
        publication: NativeTrackPublication,
    },
    /// The core published a local track again after a full reconnect. The
    /// publication replaces the one with `previous_sid`.
    LocalTrackRepublished {
        previous_sid: String,
        publication: NativeTrackPublication,
    },
    TrackMuted {
        publication: NativeTrackPublication,
    },
    TrackUnmuted {
        publication: NativeTrackPublication,
    },
    TrackSubscribed {
        publication: NativeTrackPublication,
    },
    TrackUnsubscribed {
        publication: NativeTrackPublication,
    },
    TrackSubscriptionFailed {
        participant_identity: String,
        track_sid: String,
        reason: String,
    },
    DataReceived {
        packet: NativeDataPacket,
    },
}

// Omit payloads from diagnostic formatting, including refresh tokens and RPC data.
impl std::fmt::Debug for NativeRoomEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::RpcRequested { .. } => "RpcRequested",
            Self::ByteStreamOpened { .. } => "ByteStreamOpened",
            Self::TextStreamOpened { .. } => "TextStreamOpened",
            Self::DataTrackPublished { .. } => "DataTrackPublished",
            Self::DataTrackUnpublished { .. } => "DataTrackUnpublished",
            Self::ConnectionStateChanged { .. } => "ConnectionStateChanged",
            Self::Disconnected { .. } => "Disconnected",
            Self::RoomUpdated { .. } => "RoomUpdated",
            Self::RoomMetadataChanged { .. } => "RoomMetadataChanged",
            Self::RoomMoved { .. } => "RoomMoved",
            Self::TokenRefreshed { .. } => "TokenRefreshed",
            Self::ParticipantConnected { .. } => "ParticipantConnected",
            Self::ParticipantDisconnected { .. } => "ParticipantDisconnected",
            Self::ParticipantUpdated { .. } => "ParticipantUpdated",
            Self::ParticipantMetadataChanged { .. } => "ParticipantMetadataChanged",
            Self::ParticipantNameChanged { .. } => "ParticipantNameChanged",
            Self::ParticipantAttributesChanged { .. } => "ParticipantAttributesChanged",
            Self::ParticipantPermissionChanged { .. } => "ParticipantPermissionChanged",
            Self::ParticipantEncryptionStatusChanged { .. } => "ParticipantEncryptionStatusChanged",
            Self::EncryptionStateChanged { .. } => "EncryptionStateChanged",
            Self::ConnectionQualityChanged { .. } => "ConnectionQualityChanged",
            Self::ActiveSpeakersChanged { .. } => "ActiveSpeakersChanged",
            Self::TrackPublished { .. } => "TrackPublished",
            Self::TrackUnpublished { .. } => "TrackUnpublished",
            Self::LocalTrackRepublished { .. } => "LocalTrackRepublished",
            Self::TrackMuted { .. } => "TrackMuted",
            Self::TrackUnmuted { .. } => "TrackUnmuted",
            Self::TrackSubscribed { .. } => "TrackSubscribed",
            Self::TrackUnsubscribed { .. } => "TrackUnsubscribed",
            Self::TrackSubscriptionFailed { .. } => "TrackSubscriptionFailed",
            Self::DataReceived { .. } => "DataReceived",
        };
        f.debug_struct(name).finish_non_exhaustive()
    }
}

/// A room event with the state revision that includes it.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeRoomUpdate {
    /// The revision of [`NativeRoomState`] after this event.
    pub revision: u64,
    pub event: NativeRoomEvent,
}

/// A value snapshot of a remote data track publication.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeDataTrackPublication {
    pub info: crate::data_track::common::DataTrackInfo,
    pub publisher_identity: String,
}
