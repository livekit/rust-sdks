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

use std::{collections::HashMap, fmt::Debug, sync::Arc};

use libwebrtc::enum_dispatch;
use livekit_protocol as proto;
use parking_lot::{Mutex, RwLock};

use crate::{
    observation::{Observed, StateRevision, StateSnapshot},
    prelude::*,
    rtc_engine::RtcEngine,
};

mod local_participant;
mod remote_participant;
mod rpc;

pub use local_participant::*;
pub use remote_participant::*;
pub use rpc::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParticipantState {
    Joining,
    Joined,
    Active,
    Disconnected,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParticipantKind {
    Standard,
    Ingress,
    Egress,
    Sip,
    Agent,
    Connector,
    Bridge,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ParticipantKindDetail {
    CloudAgent,
    Forwarded,
    ConnectorWhatsapp,
    ConnectorTwilio,
    BridgeRtsp,
    Simulation,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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

pub use livekit_common::ClientCapability;

#[derive(Debug, Clone)]
pub enum Participant {
    Local(LocalParticipant),
    Remote(RemoteParticipant),
}

impl Participant {
    enum_dispatch!(
        [Local, Remote];
        pub fn sid(self: &Self) -> ParticipantSid;
        pub fn identity(self: &Self) -> ParticipantIdentity;
        pub fn name(self: &Self) -> String;
        pub fn state(self: &Self) -> ParticipantState;
        pub fn metadata(self: &Self) -> String;
        pub fn attributes(self: &Self) -> HashMap<String, String>;
        pub fn is_speaking(self: &Self) -> bool;
        pub fn audio_level(self: &Self) -> f32;
        pub fn connection_quality(self: &Self) -> ConnectionQuality;
        pub fn kind(self: &Self) -> ParticipantKind;
        pub fn kind_details(self: &Self) -> Vec<ParticipantKindDetail>;
        pub fn disconnect_reason(self: &Self) -> DisconnectReason;
        pub fn joined_at(self: &Self) -> i64;
        pub fn is_encrypted(self: &Self) -> bool;
        pub fn permission(self: &Self) -> Option<proto::ParticipantPermission>;
        pub fn client_protocol(self: &Self) -> i32;

        pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) -> ();

        // Internal functions called by the Room when receiving the associated signal messages
        pub(crate) fn set_speaking(self: &Self, speaking: bool) -> ();
        pub(crate) fn set_audio_level(self: &Self, level: f32) -> ();
        pub(crate) fn set_connection_quality(self: &Self, quality: ConnectionQuality) -> ();
        pub(crate) fn update_data_encryption_status(self: &Self, is_encrypted: bool) -> ();
    );

    fn observation_inner(&self) -> &ParticipantInner {
        match self {
            Self::Local(p) => &p.inner,
            Self::Remote(p) => &p.inner,
        }
    }

    /// Captures core values and membership under their authoritative locks.
    /// Publications are live handles; their values have independent revisions.
    pub fn observed_state(&self) -> StateSnapshot<ParticipantObservation> {
        let inner = self.observation_inner();
        // Match update_encryption_status: membership precedes participant values.
        let publications = inner.track_publications.read();
        let info = inner.info.read();
        let mut ordered: Vec<_> =
            publications.iter().map(|(sid, p)| (sid.clone(), p.clone())).collect();
        ordered.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        inner
            .revision
            .snapshot(ParticipantObservation { info: info.clone(), publications: ordered })
    }

    /// Returns initial/latest core state, then waits independently from `since`.
    pub async fn state_did_change(
        &self,
        since: Option<u64>,
    ) -> Option<StateSnapshot<ParticipantObservation>> {
        if self.observation_inner().revision.wait(since).await {
            Some(self.observed_state())
        } else {
            None
        }
    }

    pub(crate) fn finish_observation(&self) {
        let inner = self.observation_inner();
        let publications = inner.track_publications.read();
        let info = inner.info.read();
        let changed = inner.revision.mark_finished();
        drop(info);
        drop(publications);
        if changed {
            inner.revision.notify();
        }
    }

    pub fn track_publications(&self) -> HashMap<TrackSid, TrackPublication> {
        match self {
            Participant::Local(p) => p.internal_track_publications(),
            Participant::Remote(p) => p.internal_track_publications(),
        }
    }
}

/// Owned core participant values. Child publications are observed separately.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticipantInfo {
    pub is_encrypted: bool,
    pub sid: ParticipantSid,
    pub identity: ParticipantIdentity,
    pub name: String,
    pub state: ParticipantState,
    pub metadata: String,
    pub attributes: HashMap<String, String>,
    pub speaking: bool,
    pub audio_level: f32,
    pub connection_quality: ConnectionQuality,
    pub kind: ParticipantKind,
    pub kind_details: Vec<ParticipantKindDetail>,
    pub disconnect_reason: DisconnectReason,
    pub joined_at: i64,
    pub permission: Option<proto::ParticipantPermission>,
    pub client_protocol: i32,
    pub capabilities: Vec<ClientCapability>,
}

type TrackMutedHandler = Box<dyn Fn(Participant, TrackPublication) + Send>;
type TrackUnmutedHandler = Box<dyn Fn(Participant, TrackPublication) + Send>;
type MetadataChangedHandler = Box<dyn Fn(Participant, String, String) + Send>;
type AttributesChangedHandler = Box<dyn Fn(Participant, HashMap<String, String>) + Send>;
type NameChangedHandler = Box<dyn Fn(Participant, String, String) + Send>;
type EncryptionStatusChangedHandler = Box<dyn Fn(Participant, bool) + Send>;
type PermissionChangedHandler =
    Box<dyn Fn(Participant, Option<proto::ParticipantPermission>) + Send>;

#[derive(Default)]
struct ParticipantEvents {
    track_muted: Mutex<Option<TrackMutedHandler>>,
    track_unmuted: Mutex<Option<TrackUnmutedHandler>>,
    metadata_changed: Mutex<Option<MetadataChangedHandler>>,
    attributes_changed: Mutex<Option<AttributesChangedHandler>>,
    name_changed: Mutex<Option<NameChangedHandler>>,
    encryption_status_changed: Mutex<Option<EncryptionStatusChangedHandler>>,
    permission_changed: Mutex<Option<PermissionChangedHandler>>,
}

pub(super) struct ParticipantInner {
    rtc_engine: Arc<RtcEngine>,
    revision: Arc<StateRevision>,
    info: Observed<ParticipantInfo>,
    track_publications: Observed<HashMap<TrackSid, TrackPublication>>,
    events: Arc<ParticipantEvents>,
    is_data_encrypted: RwLock<Option<bool>>,
}

#[derive(Clone)]
pub struct ParticipantTrackPermission {
    pub participant_identity: ParticipantIdentity,
    pub allow_all: bool,
    pub allowed_track_sids: Vec<TrackSid>,
}

pub(super) fn new_inner(
    rtc_engine: Arc<RtcEngine>,
    sid: ParticipantSid,
    identity: ParticipantIdentity,
    name: String,
    state: ParticipantState,
    metadata: String,
    attributes: HashMap<String, String>,
    kind: ParticipantKind,
    kind_details: Vec<ParticipantKindDetail>,
    joined_at: i64,
    permission: Option<proto::ParticipantPermission>,
    client_protocol: i32,
    capabilities: Vec<ClientCapability>,
) -> Arc<ParticipantInner> {
    let revision = Arc::new(StateRevision::default());
    Arc::new(ParticipantInner {
        rtc_engine,
        info: Observed::new(
            ParticipantInfo {
                is_encrypted: false,
                sid,
                identity,
                name,
                state,
                metadata,
                attributes,
                kind,
                kind_details,
                speaking: false,
                audio_level: 0.0,
                connection_quality: ConnectionQuality::Excellent,
                disconnect_reason: DisconnectReason::UnknownReason,
                joined_at,
                permission,
                client_protocol,
                capabilities,
            },
            &revision,
        ),
        track_publications: Observed::new(HashMap::new(), &revision),
        revision,
        events: Default::default(),
        is_data_encrypted: RwLock::new(None),
    })
}

pub(super) fn update_info(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    new_info: proto::ParticipantInfo,
) {
    let mut info = inner.info.write();
    if inner.revision.is_terminal() {
        return;
    }
    info.state = new_info.state().into();
    info.disconnect_reason = new_info.disconnect_reason().into();
    info.kind = new_info.kind().into();
    info.kind_details = crate::utils::convert_kind_details(&new_info.kind_details);
    info.sid = new_info.sid.try_into().unwrap();
    info.identity = new_info.identity.into();
    info.joined_at = new_info.joined_at_ms;

    let old_name = std::mem::replace(&mut info.name, new_info.name.clone());
    let old_metadata = std::mem::replace(&mut info.metadata, new_info.metadata.clone());
    let old_attributes = std::mem::replace(&mut info.attributes, new_info.attributes.clone());
    let changed_attributes =
        crate::utils::calculate_changed_attributes(old_attributes, new_info.attributes.clone());
    let old_permission = std::mem::replace(&mut info.permission, new_info.permission.clone());
    info.client_protocol = new_info.client_protocol;
    info.capabilities =
        new_info.capabilities.iter().filter_map(|&c| ClientCapability::try_from(c).ok()).collect();
    drop(info);
    if old_name != new_info.name {
        if let Some(cb) = inner.events.name_changed.lock().as_ref() {
            cb(participant.clone(), old_name, new_info.name);
        }
    }
    if old_metadata != new_info.metadata {
        if let Some(cb) = inner.events.metadata_changed.lock().as_ref() {
            cb(participant.clone(), old_metadata, new_info.metadata);
        }
    }
    if !changed_attributes.is_empty() {
        if let Some(cb) = inner.events.attributes_changed.lock().as_ref() {
            cb(participant.clone(), changed_attributes);
        }
    }
    if old_permission != new_info.permission {
        if let Some(cb) = inner.events.permission_changed.lock().as_ref() {
            cb(participant.clone(), new_info.permission);
        }
    }
}

pub(super) fn set_speaking(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    speaking: bool,
) {
    let mut info = inner.info.write();
    if !inner.revision.is_terminal() {
        info.speaking = speaking;
    }
}

pub(super) fn set_audio_level(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    audio_level: f32,
) {
    let mut info = inner.info.write();
    if !inner.revision.is_terminal() {
        info.audio_level = audio_level;
    }
}

pub(super) fn set_connection_quality(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    quality: ConnectionQuality,
) {
    let mut info = inner.info.write();
    if !inner.revision.is_terminal() {
        info.connection_quality = quality;
    }
}

pub(super) fn on_track_muted(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, TrackPublication) + Send + 'static,
) {
    *inner.events.track_muted.lock() = Some(Box::new(handler));
}

pub(super) fn on_track_unmuted(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, TrackPublication) + Send + 'static,
) {
    *inner.events.track_unmuted.lock() = Some(Box::new(handler));
}

pub(super) fn on_metadata_changed(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, String, String) + Send + 'static,
) {
    *inner.events.metadata_changed.lock() = Some(Box::new(handler));
}

pub(super) fn on_name_changed(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, String, String) + Send + 'static,
) {
    *inner.events.name_changed.lock() = Some(Box::new(handler));
}

pub(super) fn on_attributes_changed(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, HashMap<String, String>) + Send + 'static,
) {
    *inner.events.attributes_changed.lock() = Some(Box::new(handler));
}

pub(super) fn on_encryption_status_changed(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, bool) + Send + 'static,
) {
    *inner.events.encryption_status_changed.lock() = Some(Box::new(handler));
}

pub(super) fn on_permission_changed(
    inner: &Arc<ParticipantInner>,
    handler: impl Fn(Participant, Option<proto::ParticipantPermission>) + Send + 'static,
) {
    *inner.events.permission_changed.lock() = Some(Box::new(handler));
}

pub(super) fn update_encryption_status(inner: &Arc<ParticipantInner>, participant: &Participant) {
    use crate::e2ee::EncryptionType;

    let track_publications = inner.track_publications.read();
    let data_encryption_status = inner.is_data_encrypted.read();

    // Check if all track publications are encrypted
    let tracks_encrypted = !track_publications.is_empty()
        && track_publications.values().all(|pub_| pub_.encryption_type() != EncryptionType::None);

    // Overall encryption status: both tracks and data must be encrypted (if data exists)
    let is_encrypted = match *data_encryption_status {
        Some(data_encrypted) => tracks_encrypted && data_encrypted,
        None => tracks_encrypted, // No data messages yet, only consider tracks
    };

    let mut current_status = inner.info.write();
    if inner.revision.is_terminal() {
        return;
    }
    if current_status.is_encrypted != is_encrypted {
        current_status.is_encrypted = is_encrypted;
        // Release the other locks before the observed write guard wakes readers.
        drop(track_publications);
        drop(data_encryption_status);
        drop(current_status);

        if let Some(cb) = inner.events.encryption_status_changed.lock().as_ref() {
            cb(participant.clone(), is_encrypted);
        }
    }
}

pub(super) fn update_data_encryption_status(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    is_encrypted: bool,
) {
    let mut data_encryption_status = inner.is_data_encrypted.write();
    let previous_status = *data_encryption_status;

    match previous_status {
        Some(current) if current == is_encrypted => {
            // No change needed
            return;
        }
        Some(true) if !is_encrypted => {
            // Data was encrypted, now unencrypted - update immediately
            *data_encryption_status = Some(false);
        }
        Some(false) if is_encrypted => {
            // Data was unencrypted, now encrypted - but we need to keep it false
            // because once we've seen unencrypted data, participant is not fully encrypted
            return;
        }
        None => {
            // First data message - set the status
            *data_encryption_status = Some(is_encrypted);
        }
        _ => return,
    }

    drop(data_encryption_status);

    // Update overall encryption status
    update_encryption_status(inner, participant);
}

pub(super) fn remove_publication(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    sid: &TrackSid,
) -> Option<TrackPublication> {
    let mut tracks = inner.track_publications.write();
    if inner.revision.is_terminal() {
        return None;
    }
    let publication = tracks.remove(sid);
    if let Some(publication) = publication.clone() {
        // remove events
        publication.on_muted(|_| {});
        publication.on_unmuted(|_| {});
    } else {
        // shouldn't happen (internal)
        log::warn!("could not find publication to remove: {:?}", sid);
    }
    drop(tracks);

    // Update encryption status after removing publication
    update_encryption_status(inner, participant);

    publication
}

pub(super) fn add_publication(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    publication: TrackPublication,
) {
    let mut tracks = inner.track_publications.write();
    if inner.revision.is_terminal() {
        return;
    }
    tracks.insert(publication.sid(), publication.clone());

    publication.on_muted({
        let events = inner.events.clone();
        let participant = participant.clone();
        let rtc_engine = inner.rtc_engine.clone();
        move |publication| {
            if let Some(cb) = events.track_muted.lock().as_ref() {
                if !publication.is_remote() {
                    let rtc_engine = rtc_engine.clone();
                    let publication_cloned = publication.clone();
                    tokio::spawn(async move {
                        let engine_request = rtc_engine
                            .mute_track(proto::MuteTrackRequest {
                                sid: publication_cloned.sid().to_string(),
                                muted: true,
                            })
                            .await;
                        if let Err(e) = engine_request {
                            log::error!("could not mute track: {e:?}");
                        }
                    });
                }
                cb(participant.clone(), publication);
            }
        }
    });

    publication.on_unmuted({
        let events = inner.events.clone();
        let participant = participant.clone();
        let rtc_engine = inner.rtc_engine.clone();
        move |publication| {
            if let Some(cb) = events.track_unmuted.lock().as_ref() {
                if !publication.is_remote() {
                    let rtc_engine = rtc_engine.clone();
                    let publication_cloned = publication.clone();
                    tokio::spawn(async move {
                        let engine_request = rtc_engine
                            .mute_track(proto::MuteTrackRequest {
                                sid: publication_cloned.sid().to_string(),
                                muted: false,
                            })
                            .await;
                        if let Err(e) = engine_request {
                            log::error!("could not unmute track: {e:?}");
                        }
                    });
                }
                cb(participant.clone(), publication);
            }
        }
    });
    drop(tracks);

    // Update encryption status after adding publication
    update_encryption_status(inner, participant);
}

/// Participant values and ordered live publication membership, deliberately not deep-equatable.
#[derive(Clone, Debug)]
pub struct ParticipantObservation {
    /// Immutable value content, compared independently from revision.
    pub info: ParticipantInfo,
    /// Stable keys and live objects; observe each child independently.
    pub publications: Vec<(TrackSid, TrackPublication)>,
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use livekit_token::{AccessToken, VideoGrants};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires a local LiveKit server"]
    async fn core_owned_mutation_coverage_and_callbacks_outside_locks() {
        let token = AccessToken::with_api_key("devkey", "secret")
            .with_identity("core-observer")
            .with_grants(VideoGrants {
                room_join: true,
                room: format!("observation-{}", libwebrtc::native::create_random_uuid()),
                ..Default::default()
            })
            .to_jwt()
            .unwrap();
        let url = std::env::var("LK_TEST_URL").unwrap_or_else(|_| "ws://127.0.0.1:7880".into());
        let (room, _events) = crate::Room::connect(&url, &token, Default::default()).await.unwrap();
        let participant = Participant::Local(room.local_participant());
        let initial = participant.observed_state();
        let (sent, received) = std::sync::mpsc::channel();
        if let Participant::Local(local) = &participant {
            local.on_name_changed(move |p, _, _| {
                // A callback may read a complete observation. It must not run under info.write().
                sent.send(p.observed_state().value.info).unwrap();
            });
        }
        participant.update_info(proto::ParticipantInfo {
            sid: initial.value.info.sid.to_string(),
            identity: initial.value.info.identity.to_string(),
            name: "changed name".into(),
            metadata: "changed metadata".into(),
            attributes: HashMap::from([("key".into(), "value".into())]),
            state: proto::participant_info::State::Active as i32,
            kind: proto::participant_info::Kind::Agent as i32,
            joined_at_ms: 123456789,
            client_protocol: 42,
            permission: Some(proto::ParticipantPermission {
                can_publish: true,
                can_subscribe: true,
                ..Default::default()
            }),
            capabilities: vec![ClientCapability::PacketTrailer as i32],
            ..Default::default()
        });
        let callback = received.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert_eq!(callback.metadata, "changed metadata");
        assert_eq!(callback.attributes["key"], "value");
        participant.set_audio_level(0.75);
        participant.set_speaking(true);
        participant.set_connection_quality(ConnectionQuality::Poor);
        let changed = participant.state_did_change(Some(initial.revision)).await.unwrap();
        let info = changed.value.info;
        assert_eq!(info.name, "changed name");
        assert_eq!(info.state, ParticipantState::Active);
        assert_eq!(info.kind, ParticipantKind::Agent);
        assert_eq!(info.joined_at, 123456789);
        assert_eq!(info.client_protocol, 42);
        assert_eq!(info.capabilities, [ClientCapability::PacketTrailer]);
        assert!(info.permission.unwrap().can_publish);
        assert!(info.speaking);
        assert_eq!(info.audio_level, 0.75);
        assert_eq!(info.connection_quality, ConnectionQuality::Poor);
        assert_ne!(initial.value.info.name, info.name);

        let before = room.observed_state();
        room.inner.handle_room_update(proto::Room {
            name: "updated room".into(),
            metadata: "room metadata".into(),
            empty_timeout: 12,
            departure_timeout: 13,
            max_participants: 14,
            num_participants: 15,
            num_publishers: 16,
            creation_time_ms: 17,
            active_recording: true,
            ..Default::default()
        });
        room.inner.handle_data_channel_buffered_low_threshold_change(
            crate::DataPacketKind::Reliable,
            1234,
        );
        let changed = room.state_did_change(Some(before.revision)).await.unwrap();
        let info = changed.value.info;
        assert_eq!(
            (info.empty_timeout, info.departure_timeout, info.max_participants),
            (12, 13, 14)
        );
        assert_eq!((info.num_participants, info.num_publishers, info.creation_time), (15, 16, 17));
        assert_eq!(info.name, "updated room");
        assert_eq!(info.metadata, "room metadata");
        assert!(info.active_recording);
        assert_eq!(info.reliable_dc_options.buffered_amount_low_threshold, 1234);
        assert_ne!(before.value.info.name, info.name);
        room.inner.update_connection_state(crate::ConnectionState::Reconnecting);
        let reconnecting = room.observed_state();
        assert!(!reconnecting.terminal);
        room.inner.update_connection_state(crate::ConnectionState::Connected);
        assert!(room.observed_state().revision > reconnecting.revision);
        // Real writer paths contend with the three-lock snapshot. No writer may
        // retain one room lock while acquiring another in reverse order.
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..200 {
                    room.inner.handle_room_update(proto::Room::default());
                }
            });
            scope.spawn(|| {
                for _ in 0..200 {
                    room.inner.handle_speakers_changed(Vec::new());
                }
            });
            scope.spawn(|| {
                for _ in 0..200 {
                    room.inner.handle_participant_update(vec![proto::ParticipantInfo {
                        sid: "PA_lock".into(),
                        identity: "lock-probe".into(),
                        ..Default::default()
                    }]);
                    if let Some(remote) =
                        room.inner.get_participant_by_identity(&"lock-probe".into())
                    {
                        room.inner.clone().handle_participant_disconnect(remote);
                    }
                }
            });
            for _ in 0..200 {
                let _ = room.observed_state();
            }
        });
        room.close().await.unwrap();
        let final_room = room.observed_state();
        room.inner
            .handle_room_update(proto::Room { name: "too late".into(), ..Default::default() });
        room.inner.handle_data_channel_buffered_low_threshold_change(
            crate::DataPacketKind::Reliable,
            999,
        );
        room.inner.handle_speakers_changed(Vec::new());
        assert!(!room.inner.update_connection_state(crate::ConnectionState::Connected));
        assert_eq!(room.observed_state().revision, final_room.revision);
        assert_eq!(room.observed_state().value.info, final_room.value.info);
        assert!(room.state_did_change(Some(final_room.revision)).await.is_none());
        assert!(room.observed_state().terminal);
        assert!(participant.observed_state().terminal);
        let terminal = participant.observed_state();
        let late = RemoteTrackPublication::new(
            proto::TrackInfo { sid: "TR_late".into(), ..Default::default() },
            None,
            true,
        );
        add_publication(
            &room.local_participant().inner,
            &participant,
            TrackPublication::Remote(late),
        );
        assert_eq!(participant.observed_state().revision, terminal.revision);
        assert!(participant.observed_state().value.publications.is_empty());
    }
}
