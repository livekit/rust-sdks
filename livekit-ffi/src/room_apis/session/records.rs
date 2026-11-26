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
//! Core values retain their names; host SDKs qualify generated types at their compatibility boundary.

use super::errors::{NativeRequestFailureCode, NativeRtcFailureKind, NativeSignalFailure};
pub use livekit::webrtc::native::frame_cryptor::EncryptionState;
use std::{collections::HashMap, sync::Arc};

/// Native media cryptor state, forwarded from the authoritative room.
#[uniffi::remote(Enum)]
pub enum EncryptionState {
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
use livekit::prelude::*;
pub use livekit::{
    e2ee::EncryptionType,
    participant::{
        ConnectionQuality, DisconnectReason, ParticipantKind, ParticipantKindDetail,
        ParticipantState,
    },
    publication::{PermissionStatus, SubscriptionStatus},
    track::{StreamState, TrackKind, TrackSource},
    DataPacketKind,
};

#[uniffi::remote(Enum)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

#[uniffi::remote(Enum)]
pub enum DisconnectReason {
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
pub enum ParticipantKind {
    Standard,
    Ingress,
    Egress,
    Sip,
    Agent,
    Connector,
    Bridge,
}

#[uniffi::remote(Enum)]
pub enum ParticipantKindDetail {
    CloudAgent,
    Forwarded,
    ConnectorWhatsapp,
    ConnectorTwilio,
    BridgeRtsp,
    Simulation,
}

#[uniffi::remote(Enum)]
pub enum ParticipantState {
    Joining,
    Joined,
    Active,
    Disconnected,
}

#[uniffi::remote(Enum)]
pub enum SubscriptionStatus {
    Desired,
    Subscribed,
    Unsubscribed,
}

#[uniffi::remote(Enum)]
pub enum PermissionStatus {
    Allowed,
    NotAllowed,
}

#[uniffi::remote(Enum)]
pub enum TrackKind {
    Audio,
    Video,
}

#[uniffi::remote(Enum)]
pub enum TrackSource {
    Unknown,
    Camera,
    Microphone,
    Screenshare,
    ScreenshareAudio,
}

#[uniffi::remote(Enum)]
pub enum StreamState {
    Active,
    Paused,
}

#[uniffi::remote(Enum)]
pub enum DataPacketKind {
    Lossy,
    Reliable,
}

/// The lifecycle of a [`FfiRoom`](super::FfiRoom).
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
    /// Exact signaling cause when the public core retains it.
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

pub use livekit::webrtc::peer_connection_factory::IceServer;
/// Core ICE server credentials, without an intermediate record.
#[uniffi::remote(Record)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[uniffi(default = "")]
    pub username: String,
    #[uniffi(default = "")]
    pub password: String,
}

pub use livekit::webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, IceTransportsType as IceTransportPolicy,
};
/// Core ICE candidate policy.
#[uniffi::remote(Enum)]
pub enum IceTransportPolicy {
    Relay,
    NoHost,
    All,
}
/// Core ICE gathering policy.
#[uniffi::remote(Enum)]
pub enum ContinualGatheringPolicy {
    GatherOnce,
    GatherContinually,
}

/// Options for [`FfiRoom::events`](super::FfiRoom::events).
#[derive(uniffi::Record, Clone, Copy, Debug)]
pub struct NativeEventCursorOptions {
    /// Largest number of events that wait for the consumer.
    #[uniffi(default = 4096)]
    pub max_items: u32,
    /// Largest sum of payload bytes that waits for the consumer.
    #[uniffi(default = 16777216)]
    pub max_bytes: u64,
}

/// Options for [`FfiRoom::publish_data`](super::FfiRoom::publish_data).
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

pub use livekit::{DataChannelOptions, RoomInfo};

uniffi::custom_type!(ParticipantIdentity, String, {
    remote,
    lower: |identity| identity.into(),
    try_lift: |identity| Ok(identity.into()),
});
uniffi::custom_type!(TrackSid, String, {
    remote,
    lower: |sid| sid.into(),
    try_lift: |sid| TrackSid::try_from(sid).map_err(uniffi::deps::anyhow::Error::msg),
});

uniffi::custom_type!(RoomSid, String, {
    remote,
    lower: |sid| sid.into(),
    try_lift: |sid| RoomSid::try_from(sid).map_err(uniffi::deps::anyhow::Error::msg),
});

/// The core connection state, distinct from the FfiRoom lifecycle before connection.
#[uniffi::remote(Enum)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Reconnecting,
}

/// Core data channel configuration.
#[uniffi::remote(Record)]
pub struct DataChannelOptions {
    pub buffered_amount_low_threshold: u64,
}

/// Room information owned by core, including data channel options.
#[uniffi::remote(Record)]
pub struct RoomInfo {
    pub sid: Option<RoomSid>,
    pub name: String,
    pub metadata: String,
    pub state: ConnectionState,
    pub lossy_dc_options: DataChannelOptions,
    pub reliable_dc_options: DataChannelOptions,
    pub empty_timeout: u32,
    pub departure_timeout: u32,
    pub max_participants: u32,
    pub creation_time: i64,
    pub num_publishers: u32,
    pub num_participants: u32,
    pub active_recording: bool,
}

// Preserve legacy recorder/agent fields in the complete core record.
#[allow(deprecated)]
mod permissions {
    pub use livekit_protocol::ParticipantPermission;
    /// Core protocol permissions. Unknown publish-source values remain intact for host interpretation.
    #[uniffi::remote(Record)]
    pub struct ParticipantPermission {
        pub can_subscribe: bool,
        pub can_publish: bool,
        pub can_publish_data: bool,
        pub can_publish_sources: Vec<i32>,
        pub hidden: bool,
        pub recorder: bool,
        pub can_update_metadata: bool,
        pub agent: bool,
        pub can_subscribe_metrics: bool,
        pub can_manage_agent_session: bool,
    }
}
pub use permissions::ParticipantPermission;

/// A live participant view. Reads forward to core; retaining it does not retain FfiRoom.
#[derive(uniffi::Object, Clone, Debug)]
pub struct FfiParticipant(Participant);
impl From<&Participant> for FfiParticipant {
    fn from(p: &Participant) -> Self {
        Self(p.clone())
    }
}
impl PartialEq for FfiParticipant {
    fn eq(&self, other: &Self) -> bool {
        self.0.sid() == other.0.sid()
    }
}
#[uniffi::export]
impl FfiParticipant {
    /// Returns the current core identity.
    pub fn identity(&self) -> String {
        self.0.identity().into()
    }
    /// Returns the current core sid.
    pub fn sid(&self) -> String {
        self.0.sid().into()
    }
    /// Returns the current core is local.
    pub fn is_local(&self) -> bool {
        matches!(self.0, Participant::Local(_))
    }
    /// Returns the current core name.
    pub fn name(&self) -> String {
        self.0.name()
    }
    /// Returns the current core state.
    pub fn state(&self) -> ParticipantState {
        self.0.state()
    }
    /// Returns the current core kind.
    pub fn kind(&self) -> ParticipantKind {
        self.0.kind()
    }
    /// Returns the current core kind details.
    pub fn kind_details(&self) -> Vec<ParticipantKindDetail> {
        self.0.kind_details()
    }
    /// Returns the current core metadata.
    pub fn metadata(&self) -> String {
        self.0.metadata()
    }
    /// Returns the current core attributes.
    pub fn attributes(&self) -> HashMap<String, String> {
        self.0.attributes()
    }
    /// Returns the current core is speaking.
    pub fn is_speaking(&self) -> bool {
        self.0.is_speaking()
    }
    /// Returns the current core audio level.
    pub fn audio_level(&self) -> f32 {
        self.0.audio_level()
    }
    /// Returns the current core connection quality.
    pub fn connection_quality(&self) -> ConnectionQuality {
        self.0.connection_quality()
    }
    /// Returns the current core joined at.
    pub fn joined_at(&self) -> i64 {
        self.0.joined_at()
    }
    /// Returns the current core permission.
    pub fn permission(&self) -> Option<ParticipantPermission> {
        self.0.permission()
    }
    /// Returns the current core is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.0.is_encrypted()
    }
    /// Returns the current core disconnect reason.
    pub fn disconnect_reason(&self) -> DisconnectReason {
        self.0.disconnect_reason()
    }
    /// Returns the current core client protocol.
    pub fn client_protocol(&self) -> i32 {
        self.0.client_protocol()
    }
    /// Returns the current core capabilities.
    pub fn capabilities(&self) -> Option<Vec<crate::data_stream::common::ClientCapability>> {
        match &self.0 {
            Participant::Remote(p) => Some(p.capabilities().into_iter().map(Into::into).collect()),
            Participant::Local(_) => None,
        }
    }
}

/// The part of a publication that only a remote track has.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct NativeRemotePublicationState {
    pub subscription_status: SubscriptionStatus,
    pub permission_status: PermissionStatus,
    /// `false` when the receiver paused the track.
    pub is_enabled: bool,
    /// `None` when the track is not subscribed.
    pub stream_state: Option<StreamState>,
}

/// A live publication view, retaining the core publication rather than a copied snapshot.
#[derive(uniffi::Object, Clone, Debug)]
pub struct FfiTrackPublication {
    participant_identity: String,
    core: TrackPublication,
}
impl PartialEq for FfiTrackPublication {
    fn eq(&self, other: &Self) -> bool {
        self.core.sid() == other.core.sid()
            && self.participant_identity == other.participant_identity
    }
}
impl FfiTrackPublication {
    pub(super) fn new(identity: &str, publication: &TrackPublication) -> Arc<Self> {
        Arc::new(Self { participant_identity: identity.into(), core: publication.clone() })
    }
}
#[uniffi::export]
impl FfiTrackPublication {
    /// Returns the current core sid.
    pub fn sid(&self) -> String {
        self.core.sid().into()
    }
    /// Returns the current core participant identity.
    pub fn participant_identity(&self) -> String {
        self.participant_identity.clone()
    }
    /// Returns the current core name.
    pub fn name(&self) -> String {
        self.core.name()
    }
    /// Returns the current core kind.
    pub fn kind(&self) -> TrackKind {
        self.core.kind()
    }
    /// Returns the current core source.
    pub fn source(&self) -> TrackSource {
        self.core.source()
    }
    /// Returns the current core is muted.
    pub fn is_muted(&self) -> bool {
        self.core.is_muted()
    }
    /// Returns the current core simulcasted.
    pub fn simulcasted(&self) -> bool {
        self.core.simulcasted()
    }
    /// Returns the current core width.
    pub fn width(&self) -> u32 {
        self.core.dimension().0
    }
    /// Returns the current core height.
    pub fn height(&self) -> u32 {
        self.core.dimension().1
    }
    /// Returns the current core mime type.
    pub fn mime_type(&self) -> String {
        self.core.mime_type()
    }
    /// Returns the current core encryption type.
    pub fn encryption_type(&self) -> EncryptionType {
        self.core.encryption_type()
    }
    /// Returns the current core remote.
    pub fn remote(&self) -> Option<NativeRemotePublicationState> {
        match &self.core {
            TrackPublication::Local(_) => None,
            TrackPublication::Remote(p) => Some(NativeRemotePublicationState {
                subscription_status: p.subscription_status(),
                permission_status: p.permission_status(),
                is_enabled: p.is_enabled(),
                stream_state: p.track().map(|t| t.stream_state()),
            }),
        }
    }
}

/// Ordered session membership at a revision, with live core-backed object properties.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct FfiRoomState {
    /// Remote data track publications known at this revision.
    pub data_tracks: Vec<Arc<crate::data_track::remote::RemoteDataTrack>>,
    /// Position in the ordered event stream; core values are read on demand.
    pub revision: u64,
    pub connection: NativeConnectionState,
    /// `None` until the session is `Disconnected`.
    pub disconnect_reason: Option<DisconnectReason>,
    /// `None` before the first connection.
    pub room: Option<RoomInfo>,
    pub local_participant: Option<Arc<FfiParticipant>>,
    /// In no particular order.
    pub remote_participants: Vec<Arc<FfiParticipant>>,
    /// Local and remote publications, in no particular order.
    pub publications: Vec<Arc<FfiTrackPublication>>,
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
    pub kind: DataPacketKind,
    /// `None` when the server sent the packet.
    pub participant_identity: Option<String>,
}

/// An ordered room event. Core-backed object properties are read live; owned values
/// describe the event. Membership ordering is preserved even when properties advance.
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
        reason: DisconnectReason,
    },
    /// Counters or recording state changed. Only the newest value is kept.
    RoomUpdated {
        room: RoomInfo,
    },
    RoomMetadataChanged {
        room: RoomInfo,
        old_metadata: String,
    },
    RoomMoved {
        room: RoomInfo,
    },
    TokenRefreshed {
        token: String,
    },
    ParticipantConnected {
        participant: Arc<FfiParticipant>,
    },
    ParticipantDisconnected {
        participant: Arc<FfiParticipant>,
    },
    /// A change that has no specific event. Only the newest value is kept.
    ParticipantUpdated {
        participant: Arc<FfiParticipant>,
    },
    ParticipantMetadataChanged {
        participant: Arc<FfiParticipant>,
        old_metadata: String,
    },
    ParticipantNameChanged {
        participant: Arc<FfiParticipant>,
        old_name: String,
    },
    ParticipantAttributesChanged {
        participant: Arc<FfiParticipant>,
        changed_attributes: HashMap<String, String>,
    },
    ParticipantPermissionChanged {
        participant: Arc<FfiParticipant>,
    },
    ParticipantEncryptionStatusChanged {
        participant: Arc<FfiParticipant>,
    },
    EncryptionStateChanged {
        participant: Arc<FfiParticipant>,
        state: EncryptionState,
    },
    /// Only the newest value for each participant is kept.
    ConnectionQualityChanged {
        participant: Arc<FfiParticipant>,
    },
    /// A participant that is absent from `speakers` stopped speaking. Only
    /// the newest value is kept.
    ActiveSpeakersChanged {
        speakers: Vec<NativeSpeaker>,
    },
    TrackPublished {
        publication: Arc<FfiTrackPublication>,
    },
    TrackUnpublished {
        publication: Arc<FfiTrackPublication>,
    },
    /// The core published a local track again after a full reconnect. The
    /// publication replaces the one with `previous_sid`.
    LocalTrackRepublished {
        previous_sid: String,
        publication: Arc<FfiTrackPublication>,
    },
    TrackMuted {
        publication: Arc<FfiTrackPublication>,
    },
    TrackUnmuted {
        publication: Arc<FfiTrackPublication>,
    },
    TrackSubscribed {
        publication: Arc<FfiTrackPublication>,
    },
    TrackUnsubscribed {
        publication: Arc<FfiTrackPublication>,
    },
    TrackSubscriptionFailed {
        participant_identity: String,
        track_sid: String,
        reason: String,
    },
    DataReceived {
        packet: NativeDataPacket,
    },
    /// A DTMF packet received through the core.
    DtmfReceived {
        code: u32,
        digit: Option<String>,
        participant_identity: Option<String>,
    },
}

// Omit payloads from diagnostic formatting, including refresh tokens and RPC data.
impl std::fmt::Debug for NativeRoomEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::DtmfReceived { .. } => "DtmfReceived",
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
    /// The revision of [`FfiRoomState`] after this event.
    pub revision: u64,
    pub event: NativeRoomEvent,
}
