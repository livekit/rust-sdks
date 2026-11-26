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
use super::{FfiParticipant, FfiTrack, FfiTrackPublication};
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
/// `Disconnected` is final. A new connection needs a new room.
// This is room state, not a lossy replacement for core ConnectionState (also
// exported directly below): Idle/Connecting precede any core Room, and terminal
// Disconnected belongs to the room after the Room has gone away.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeConnectionState {
    /// The room exists and `connect` was not called.
    Idle,
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
}

/// The error type of the typed room surface.
#[derive(uniffi::Error, thiserror::Error, Clone, Debug, PartialEq)]
pub enum FfiRoomError {
    /// `cancel` ended the room or the cursor.
    #[error("cancelled")]
    Cancelled,
    /// The room ended. The cause is a disconnect or a failed connection.
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

/// Returns `true` when `error` means that the room or cursor ended.
///
/// The classification stays in Rust so that every binding agrees on it.
#[uniffi::export]
pub fn ffi_room_error_is_shutdown(error: FfiRoomError) -> bool {
    matches!(error, FfiRoomError::Cancelled | FfiRoomError::Closed)
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
// Core takes DataPacket including payload; this existing command splits payload
// into its own argument. Reliable maps to the two DataPacketKind variants and
// destination strings become ParticipantIdentity without changing their values.
// A remote DataPacket is possible; retaining this shape is API compatibility.
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

/// A publication relationship, carrying both original core objects without a snapshot.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiPublication {
    pub participant: Arc<FfiParticipant>,
    pub publication: Arc<FfiTrackPublication>,
}
impl FfiPublication {
    pub(super) fn new(participant: &Participant, publication: &TrackPublication) -> Self {
        Self {
            participant: Arc::new(participant.clone()),
            publication: Arc::new(publication.clone()),
        }
    }
}

/// Ordered room membership at a revision, with live core-backed object properties.
// Core has no cursor-revision record. This aggregates actual RoomInfo and core
// handles with receiver-ordered membership; it does not copy their properties
// into mobile snapshots. Live property reads may be newer than this revision.
#[derive(uniffi::Record, Clone, Debug)]
pub struct FfiRoomState {
    /// Remote data track publications known at this revision.
    pub data_tracks: Vec<Arc<crate::data_track::remote::RemoteDataTrack>>,
    /// Position in the ordered event stream; core values are read on demand.
    pub revision: u64,
    pub connection: NativeConnectionState,
    /// `None` until the room is `Disconnected`.
    pub disconnect_reason: Option<DisconnectReason>,
    /// `None` before the first connection.
    pub room: Option<RoomInfo>,
    pub local_participant: Option<Arc<FfiParticipant>>,
    /// In no particular order.
    pub remote_participants: Vec<Arc<FfiParticipant>>,
    /// Local and remote publications, in no particular order.
    pub publications: Vec<FfiPublication>,
}

/// A data packet from a remote participant.
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeDataPacket {
    pub payload: Vec<u8>,
    pub topic: Option<String>,
    pub kind: DataPacketKind,
    /// `None` when the server sent the packet.
    pub participant: Option<Arc<FfiParticipant>>,
}

/// An ordered room event. Core-backed object properties are read live; owned values
/// describe the event. Membership ordering is preserved even when properties advance.
///
// Core RoomEvent contains tuple collections, Arc payloads and core stream/media
// handles without converters for that exact event representation. This cursor
// protocol also adds RPC requests and room lifecycle events. Preserve owned
// event fields and actual core entity handles here; mobile snapshot conversion
// belongs in the consumer. See the error/media adapters for their stated losses.
/// New variants can appear in a later version, so a consumer must ignore a
/// variant that it does not know.
#[derive(uniffi::Enum, Clone)]
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
    /// The last event of a room.
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
    ParticipantActive {
        participant: Arc<FfiParticipant>,
    },
    ParticipantsUpdated {
        participants: Vec<Arc<FfiParticipant>>,
    },
    ParticipantMetadataChanged {
        participant: Arc<FfiParticipant>,
        old_metadata: String,
        metadata: String,
    },
    ParticipantNameChanged {
        participant: Arc<FfiParticipant>,
        old_name: String,
        name: String,
    },
    ParticipantAttributesChanged {
        participant: Arc<FfiParticipant>,
        changed_attributes: HashMap<String, String>,
    },
    ParticipantPermissionChanged {
        participant: Arc<FfiParticipant>,
        permission: Option<ParticipantPermission>,
    },
    ParticipantEncryptionStatusChanged {
        participant: Arc<FfiParticipant>,
        is_encrypted: bool,
    },
    EncryptionStateChanged {
        participant: Arc<FfiParticipant>,
        state: EncryptionState,
    },
    /// Only the newest value for each participant is kept.
    ConnectionQualityChanged {
        participant: Arc<FfiParticipant>,
        quality: ConnectionQuality,
    },
    /// A participant that is absent from `speakers` stopped speaking. Only
    /// the newest value is kept.
    ActiveSpeakersChanged {
        speakers: Vec<Arc<FfiParticipant>>,
    },
    TrackPublished {
        publication: FfiPublication,
    },
    LocalTrackPublished {
        publication: FfiPublication,
        track: Arc<FfiTrack>,
    },
    TrackUnpublished {
        publication: FfiPublication,
    },
    /// The core published a local track again after a full reconnect. The
    /// publication replaces the one with `previous_sid`.
    LocalTrackRepublished {
        previous_sid: String,
        publication: FfiPublication,
        track: Arc<FfiTrack>,
    },
    TrackMuted {
        publication: FfiPublication,
    },
    TrackUnmuted {
        publication: FfiPublication,
    },
    TrackSubscribed {
        publication: FfiPublication,
        track: Arc<FfiTrack>,
    },
    TrackUnsubscribed {
        publication: FfiPublication,
        track: Arc<FfiTrack>,
    },
    TrackSubscriptionFailed {
        participant: Arc<FfiParticipant>,
        track_sid: TrackSid,
        error: TrackError,
    },
    DataReceived {
        packet: NativeDataPacket,
    },
    /// A DTMF packet received through the core.
    DtmfReceived {
        code: u32,
        digit: Option<String>,
        participant: Option<Arc<FfiParticipant>>,
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
            Self::ParticipantActive { .. } => "ParticipantActive",
            Self::ParticipantsUpdated { .. } => "ParticipantsUpdated",
            Self::ParticipantMetadataChanged { .. } => "ParticipantMetadataChanged",
            Self::ParticipantNameChanged { .. } => "ParticipantNameChanged",
            Self::ParticipantAttributesChanged { .. } => "ParticipantAttributesChanged",
            Self::ParticipantPermissionChanged { .. } => "ParticipantPermissionChanged",
            Self::ParticipantEncryptionStatusChanged { .. } => "ParticipantEncryptionStatusChanged",
            Self::EncryptionStateChanged { .. } => "EncryptionStateChanged",
            Self::ConnectionQualityChanged { .. } => "ConnectionQualityChanged",
            Self::ActiveSpeakersChanged { .. } => "ActiveSpeakersChanged",
            Self::TrackPublished { .. } => "TrackPublished",
            Self::LocalTrackPublished { .. } => "LocalTrackPublished",
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
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeRoomUpdate {
    /// The revision of [`FfiRoomState`] after this event.
    pub revision: u64,
    pub event: NativeRoomEvent,
}

/// Core publication audio features, without classification loss.
pub use livekit_protocol::{AudioTrackFeature, PacketTrailerFeature};
#[uniffi::remote(Enum)]
pub enum AudioTrackFeature {
    TfStereo,
    TfNoDtx,
    TfAutoGainControl,
    TfEchoCancellation,
    TfNoiseSuppression,
    TfEnhancedNoiseCancellation,
    TfPreconnectBuffer,
}
/// Core packet trailer features.
#[uniffi::remote(Enum)]
pub enum PacketTrailerFeature {
    PtfUserTimestamp,
    PtfFrameId,
    PtfUserData,
}

uniffi::custom_type!(ParticipantSid, String, {
    remote,
    lower: |sid| sid.into(),
    try_lift: |sid| ParticipantSid::try_from(sid).map_err(uniffi::deps::anyhow::Error::msg),
});

/// Core subscription failure, retaining its structured track SID.
pub use livekit::track::TrackError;
#[uniffi::remote(Error)]
pub enum TrackError {
    TrackNotFound(TrackSid),
}
