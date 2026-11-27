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

//! Concrete UniFFI observations over core-owned values. Native WebRTC queries
//! remain on the Rust objects, outside revision/equality; Swift does not own them.
use super::*;
use livekit::{
    observation::StateSnapshot,
    participant::{ParticipantInfo, ParticipantObservation},
    publication::PublicationObservation,
    track::TrackValues,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Core participant information is already an owned value: no mobile projection in Rust.
#[uniffi::remote(Record)]
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
    pub permission: Option<ParticipantPermission>,
    pub client_protocol: i32,
    pub capabilities: Vec<livekit_common::ClientCapability>,
}

/// Core track values exclude enabled and receiver codec parameters (live queries).
#[uniffi::remote(Record)]
pub struct TrackValues {
    pub sid: TrackSid,
    pub name: String,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub stream_state: StreamState,
    pub muted: bool,
    pub audio_features: Vec<livekit_protocol::AudioTrackFeature>,
}

/// UniFFI needs named scalar dimensions rather than the core tuple/object wrapper.
/// Every other owned value is retained without mobile/v2 conversion.
#[derive(Clone, uniffi::Record)]
pub struct NativePublicationValues {
    pub sid: TrackSid,
    pub name: String,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub simulcasted: bool,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub muted: bool,
    pub encryption_type: EncryptionType,
    pub audio_features: Vec<livekit_protocol::AudioTrackFeature>,
    pub frame_metadata_features: Vec<livekit_protocol::PacketTrailerFeature>,
    pub desired: Option<bool>,
    pub allowed: Option<bool>,
    pub publish_options: Option<livekit::options::TrackPublishOptions>,
}

/// Owned content and live children are deliberately separate; compare `values`, not revisions.
#[derive(Clone, uniffi::Record)]
pub struct NativeParticipantState {
    pub revision: u64,
    pub terminal: bool,
    pub values: ParticipantInfo,
    /// Deterministically ordered live children; no deep equality is promised.
    pub publications: Vec<Arc<FfiTrackPublication>>,
}
impl From<StateSnapshot<ParticipantObservation>> for NativeParticipantState {
    fn from(s: StateSnapshot<ParticipantObservation>) -> Self {
        Self {
            revision: s.revision,
            terminal: s.terminal,
            values: s.value.info,
            publications: s.value.publications.into_iter().map(|(_, p)| Arc::new(p)).collect(),
        }
    }
}

/// Owned publication values and its independently observed live media child.
#[derive(Clone, uniffi::Record)]
pub struct NativePublicationState {
    pub revision: u64,
    pub terminal: bool,
    pub values: NativePublicationValues,
    pub track: Option<Arc<FfiTrack>>,
}
impl From<StateSnapshot<PublicationObservation>> for NativePublicationState {
    fn from(s: StateSnapshot<PublicationObservation>) -> Self {
        let v = s.value.values;
        Self {
            revision: s.revision,
            terminal: s.terminal,
            values: NativePublicationValues {
                sid: v.sid,
                name: v.name,
                kind: v.kind,
                source: v.source,
                simulcasted: v.simulcasted,
                width: v.dimension.0,
                height: v.dimension.1,
                mime_type: v.mime_type,
                muted: v.muted,
                encryption_type: v.encryption_type,
                audio_features: v.audio_features,
                frame_metadata_features: v.frame_metadata_features,
                desired: v.desired,
                allowed: v.allowed,
                publish_options: v.publish_options,
            },
            track: s.value.track.map(Arc::new),
        }
    }
}

/// Immutable core track values, compared independently from the revision.
#[derive(Clone, uniffi::Record)]
pub struct NativeTrackState {
    pub revision: u64,
    pub terminal: bool,
    pub values: TrackValues,
}
impl From<StateSnapshot<TrackValues>> for NativeTrackState {
    fn from(s: StateSnapshot<TrackValues>) -> Self {
        Self { revision: s.revision, terminal: s.terminal, values: s.value }
    }
}

// UniFFI cannot export generic objects. The only per-observer resource is a
// cancellation token: no task, state cache, baseline, channel or event queue.
macro_rules! observer {
    ($name:ident, $core:ty, $value:ty) => {
        /// Independent cancellable observation; retaining it retains its source object.
        #[derive(uniffi::Object)]
        pub struct $name {
            source: Arc<$core>,
            cancelled: CancellationToken,
        }
        impl $name {
            pub(super) fn new(source: Arc<$core>) -> Arc<Self> {
                Arc::new(Self { source, cancelled: CancellationToken::new() })
            }
        }
        #[uniffi::export]
        impl $name {
            /// Copies current core values; live children are separately observed.
            pub fn state(&self) -> $value {
                self.source.observed_state().into()
            }
            /// `None` starts immediately; unseen terminal state precedes EOF.
            pub async fn state_did_change(&self, since: Option<u64>) -> Option<$value> {
                tokio::select! { biased;
                    _ = self.cancelled.cancelled() => None,
                    state = self.source.state_did_change(since) => state.map(Into::into),
                }
            }
            /// Ends only this observer. Swift must defer this outside its cancellation handler.
            pub fn cancel(&self) {
                self.cancelled.cancel();
            }
        }
        impl Drop for $name {
            fn drop(&mut self) {
                self.cancelled.cancel();
            }
        }
    };
}
observer!(NativeParticipantObserver, FfiParticipant, NativeParticipantState);
observer!(NativePublicationObserver, FfiTrackPublication, NativePublicationState);
observer!(NativeTrackObserver, FfiTrack, NativeTrackState);

/// FfiRoom owns preconnect/terminal lifecycle; RoomInfo is the original core value.
#[derive(Clone, uniffi::Record)]
pub struct NativeRoomValues {
    pub connection: NativeConnectionState,
    pub disconnect_reason: Option<DisconnectReason>,
    pub room: Option<RoomInfo>,
}

/// Room values and live membership, not an atomic snapshot of the child graph.
#[derive(Clone, uniffi::Record)]
pub struct NativeRoomObservation {
    pub revision: u64,
    pub terminal: bool,
    pub values: NativeRoomValues,
    pub local_participant: Option<Arc<FfiParticipant>>,
    pub remote_participants: Vec<Arc<FfiParticipant>>,
    pub active_speakers: Vec<Arc<FfiParticipant>>,
    pub data_tracks: Vec<Arc<crate::data_track::remote::RemoteDataTrack>>,
}

impl Inner {
    fn observation(&self) -> (NativeRoomObservation, Option<(Arc<Room>, u64)>) {
        let core = self.room.as_ref().map(|room| (room.clone(), room.observed_state()));
        let core_revision = core.as_ref().map_or(self.retired_core_revision, |(_, s)| s.revision);
        let mut data_tracks: Vec<_> = self.data_tracks.iter().collect();
        data_tracks.sort_by(|a, b| a.0.cmp(b.0));
        let result = NativeRoomObservation {
            // Both counters belong to this one FfiRoom, never reset, and cannot
            // change while their respective snapshot locks are held. Core's final
            // revision is retained when its owning handle is released on shutdown.
            revision: self.revision.checked_add(core_revision).expect("room revision exhausted"),
            terminal: self.finished,
            values: NativeRoomValues {
                connection: self.connection,
                disconnect_reason: self.disconnect_reason,
                room: core
                    .as_ref()
                    .map(|(_, s)| s.value.info.clone())
                    .or_else(|| self.final_room_info.clone()),
            },
            local_participant: self
                .room
                .as_ref()
                .map(|room| Arc::new(Participant::Local(room.local_participant()))),
            remote_participants: core
                .as_ref()
                .map(|(_, s)| {
                    s.value
                        .remote_participants
                        .iter()
                        .map(|(_, p)| Arc::new(Participant::Remote(p.clone())))
                        .collect()
                })
                .unwrap_or_default(),
            active_speakers: core
                .as_ref()
                .map(|(_, s)| s.value.active_speakers.iter().cloned().map(Arc::new).collect())
                .unwrap_or_default(),
            data_tracks: data_tracks.into_iter().map(|(_, track)| track.clone()).collect(),
        };
        (result, core.map(|(room, snapshot)| (room, snapshot.revision)))
    }
}

/// A cancellable room observation, independent from event cursors and other observers.
#[derive(uniffi::Object)]
pub struct NativeRoomObserver {
    source: Arc<FfiRoom>,
    cancelled: CancellationToken,
}
#[uniffi::export]
impl NativeRoomObserver {
    /// Captures values plus live membership. Child objects carry their own revisions.
    pub fn state(&self) -> NativeRoomObservation {
        self.source.shared.inner.lock().observation().0
    }

    /// Returns initial/latest state; after its final revision, returns EOF.
    pub async fn state_did_change(&self, since: Option<u64>) -> Option<NativeRoomObservation> {
        loop {
            // Register before either snapshot read, including before a core Room exists.
            let changed = self.source.shared.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.cancelled.is_cancelled() {
                return None;
            }
            let (state, core) = self.source.shared.inner.lock().observation();
            if since != Some(state.revision) {
                return Some(state);
            }
            if state.terminal {
                return None;
            }
            let core_changed = async {
                if let Some((room, revision)) = core {
                    if room.state_did_change(Some(revision)).await.is_some() {
                        return;
                    }
                }
                // Core EOF is followed by the FFI driver's final lifecycle commit.
                std::future::pending::<()>().await;
            };
            tokio::select! { biased;
                _ = self.cancelled.cancelled() => return None,
                _ = changed => (),
                _ = core_changed => (),
            }
        }
    }

    /// Ends only this observer; defer from Swift's cancellation handler (UniFFI 0.31).
    pub fn cancel(&self) {
        self.cancelled.cancel();
    }
}
impl Drop for NativeRoomObserver {
    fn drop(&mut self) {
        self.cancelled.cancel();
    }
}

#[uniffi::export]
impl FfiRoom {
    /// Creates an independent pull observer; no subscriber task or state queue is allocated.
    pub fn observe_state(self: Arc<Self>) -> Arc<NativeRoomObserver> {
        Arc::new(NativeRoomObserver { source: self, cancelled: CancellationToken::new() })
    }
}
