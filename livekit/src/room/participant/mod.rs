// Copyright 2023 LiveKit, Inc.
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

use livekit_protocol as proto;
use livekit_protocol::enum_dispatch;
use parking_lot::{Mutex, RwLock};

use crate::{prelude::*, rtc_engine::RtcEngine};

mod local_participant;
mod remote_participant;

pub use local_participant::*;
pub use remote_participant::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

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
        pub fn metadata(self: &Self) -> String;
        pub fn is_speaking(self: &Self) -> bool;
        pub fn audio_level(self: &Self) -> f32;
        pub fn connection_quality(self: &Self) -> ConnectionQuality;

        pub(crate) fn update_info(self: &Self, info: proto::ParticipantInfo) -> ();

        // Internal functions called by the Room when receiving the associated signal messages
        pub(crate) fn set_speaking(self: &Self, speaking: bool) -> ();
        pub(crate) fn set_audio_level(self: &Self, level: f32) -> ();
        pub(crate) fn set_connection_quality(self: &Self, quality: ConnectionQuality) -> ();
        pub(crate) fn add_publication(self: &Self, publication: TrackPublication) -> ();
        pub(crate) fn remove_publication(self: &Self, sid: &TrackSid) -> Option<TrackPublication>;
    );

    pub fn track_publications(&self) -> HashMap<TrackSid, TrackPublication> {
        match self {
            Participant::Local(p) => p.internal_track_publications(),
            Participant::Remote(p) => p.internal_track_publications(),
        }
    }
}

struct ParticipantInfo {
    pub sid: ParticipantSid,
    pub identity: ParticipantIdentity,
    pub name: String,
    pub metadata: String,
    pub speaking: bool,
    pub audio_level: f32,
    pub connection_quality: ConnectionQuality,
}

type TrackMutedHandler = Box<dyn Fn(Participant, TrackPublication) + Send>;
type TrackUnmutedHandler = Box<dyn Fn(Participant, TrackPublication) + Send>;
type MetadataChangedHandler = Box<dyn Fn(Participant, String, String) + Send>;
type NameChangedHandler = Box<dyn Fn(Participant, String, String) + Send>;

#[derive(Default)]
struct ParticipantEvents {
    track_muted: Mutex<Option<TrackMutedHandler>>,
    track_unmuted: Mutex<Option<TrackUnmutedHandler>>,
    metadata_changed: Mutex<Option<MetadataChangedHandler>>,
    name_changed: Mutex<Option<NameChangedHandler>>,
}

pub(super) struct ParticipantInner {
    rtc_engine: Arc<RtcEngine>,
    info: RwLock<ParticipantInfo>,
    track_publications: RwLock<HashMap<TrackSid, TrackPublication>>,
    events: Arc<ParticipantEvents>,
}

pub(super) fn new_inner(
    rtc_engine: Arc<RtcEngine>,
    sid: ParticipantSid,
    identity: ParticipantIdentity,
    name: String,
    metadata: String,
) -> Arc<ParticipantInner> {
    Arc::new(ParticipantInner {
        rtc_engine,
        info: RwLock::new(ParticipantInfo {
            sid,
            identity,
            name,
            metadata,
            speaking: false,
            audio_level: 0.0,
            connection_quality: ConnectionQuality::Excellent,
        }),
        track_publications: Default::default(),
        events: Default::default(),
    })
}

pub(super) fn update_info(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    new_info: proto::ParticipantInfo,
) {
    let mut info = inner.info.write();
    info.sid = new_info.sid.try_into().unwrap();
    info.identity = new_info.identity.into();

    let old_name = std::mem::replace(&mut info.name, new_info.name.clone());
    if old_name != new_info.name {
        if let Some(cb) = inner.events.name_changed.lock().as_ref() {
            cb(participant.clone(), old_name, new_info.name);
        }
    }

    let old_metadata = std::mem::replace(&mut info.metadata, new_info.metadata.clone());
    if old_metadata != new_info.metadata {
        if let Some(cb) = inner.events.metadata_changed.lock().as_ref() {
            cb(participant.clone(), old_metadata, new_info.metadata);
        }
    }
}

pub(super) fn set_speaking(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    speaking: bool,
) {
    inner.info.write().speaking = speaking;
}

pub(super) fn set_audio_level(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    audio_level: f32,
) {
    inner.info.write().audio_level = audio_level;
}

pub(super) fn set_connection_quality(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    quality: ConnectionQuality,
) {
    inner.info.write().connection_quality = quality;
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

pub(super) fn remove_publication(
    inner: &Arc<ParticipantInner>,
    _participant: &Participant,
    sid: &TrackSid,
) -> Option<TrackPublication> {
    let mut tracks = inner.track_publications.write();
    let publication = tracks.remove(sid);
    if let Some(publication) = publication.clone() {
        // remove events
        publication.on_muted(|_| {});
        publication.on_unmuted(|_| {});
    } else {
        // shouldn't happen (internal)
        log::warn!("could not find publication to remove: {:?}", sid);
    }

    publication
}

pub(super) fn add_publication(
    inner: &Arc<ParticipantInner>,
    participant: &Participant,
    publication: TrackPublication,
) {
    let mut tracks = inner.track_publications.write();
    tracks.insert(publication.sid(), publication.clone());

    publication.on_muted({
        let events = inner.events.clone();
        let participant = participant.clone();
        move |publication| {
            if let Some(cb) = events.track_muted.lock().as_ref() {
                cb(participant.clone(), publication);
            }
        }
    });

    publication.on_unmuted({
        let events = inner.events.clone();
        let participant = participant.clone();
        move |publication| {
            if let Some(cb) = events.track_unmuted.lock().as_ref() {
                cb(participant.clone(), publication);
            }
        }
    });
}
