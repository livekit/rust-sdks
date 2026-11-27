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

use crate::{
    observation::{Observed, StateRevision, StateSnapshot},
    options::TrackPublishOptions,
};
use libwebrtc::enum_dispatch;
use livekit_protocol::{self as proto, AudioTrackFeature, PacketTrailerFeature};
use parking_lot::Mutex;

use super::track::TrackDimension;
use crate::{e2ee::EncryptionType, prelude::*, track::Track};

mod local;
mod remote;

pub use local::*;
pub use remote::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatus {
    Desired,
    Subscribed,
    Unsubscribed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    Allowed,
    NotAllowed,
}

#[derive(Clone, Debug)]
pub enum TrackPublication {
    Local(LocalTrackPublication),
    Remote(RemoteTrackPublication),
}

impl TrackPublication {
    enum_dispatch!(
        [Local, Remote];
        pub fn sid(self: &Self) -> TrackSid;
        pub fn name(self: &Self) -> String;
        pub fn kind(self: &Self) -> TrackKind;
        pub fn source(self: &Self) -> TrackSource;
        pub fn simulcasted(self: &Self) -> bool;
        pub fn dimension(self: &Self) -> TrackDimension;
        pub fn mime_type(self: &Self) -> String;
        pub fn is_muted(self: &Self) -> bool;
        pub fn is_remote(self: &Self) -> bool;
        pub fn encryption_type(self: &Self) -> EncryptionType;
        pub fn audio_features(self: &Self) -> Vec<AudioTrackFeature>;
        pub fn frame_metadata_features(self: &Self) -> Vec<PacketTrailerFeature>;

        pub(crate) fn on_muted(self: &Self, on_mute: impl Fn(TrackPublication) + Send + 'static) -> ();
        pub(crate) fn on_unmuted(self: &Self, on_unmute: impl Fn(TrackPublication) + Send + 'static) -> ();
        pub(crate) fn proto_info(self: &Self) -> proto::TrackInfo;
        pub(crate) fn update_info(self: &Self, info: proto::TrackInfo) -> ();
    );

    fn observation_inner(&self) -> &TrackPublicationInner {
        match self {
            Self::Local(p) => &p.inner,
            Self::Remote(p) => &p.inner,
        }
    }

    /// Owns publication values, but retains its track as an independently observed live child.
    pub fn observed_state(&self) -> StateSnapshot<PublicationObservation> {
        let inner = self.observation_inner();
        let info = inner.info.read();
        inner.revision.snapshot(PublicationObservation {
            values: PublicationValues {
                sid: info.sid.clone(),
                name: info.name.clone(),
                kind: info.kind,
                source: info.source,
                simulcasted: info.simulcasted,
                dimension: info.dimension,
                mime_type: info.mime_type.clone(),
                // Stored publication mute; effective local mute belongs to the live child track.
                muted: info.muted,
                encryption_type: info.encryption_type,
                audio_features: info.audio_features.clone(),
                frame_metadata_features: info.frame_metadata_features.clone(),
                desired: info.desired,
                allowed: info.allowed,
                publish_options: info.publish_options.clone(),
            },
            track: info.track.clone(),
        })
    }

    /// Returns initial/latest state or EOF after the terminal revision was consumed.
    pub async fn state_did_change(
        &self,
        since: Option<u64>,
    ) -> Option<StateSnapshot<PublicationObservation>> {
        if self.observation_inner().revision.wait(since).await {
            Some(self.observed_state())
        } else {
            None
        }
    }

    pub(crate) fn finish_observation(&self) {
        let inner = self.observation_inner();
        let info = inner.info.read();
        let changed = inner.revision.mark_finished();
        drop(info);
        if changed {
            inner.revision.notify();
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_track(&self, track: Option<Track>) {
        match self {
            TrackPublication::Local(p) => p.set_track(track),
            TrackPublication::Remote(p) => p.set_track(track.map(|t| t.try_into().unwrap())),
        }
    }

    pub fn track(&self) -> Option<Track> {
        match self {
            TrackPublication::Local(p) => p.track().map(Into::into),
            TrackPublication::Remote(p) => p.track().map(Into::into),
        }
    }
}

struct PublicationInfo {
    desired: Option<bool>,
    allowed: Option<bool>,
    publish_options: Option<TrackPublishOptions>,
    pub track: Option<Track>,
    pub name: String,
    pub sid: TrackSid,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub simulcasted: bool,
    pub dimension: TrackDimension,
    pub mime_type: String,
    pub muted: bool,
    pub proto_info: proto::TrackInfo,
    pub encryption_type: EncryptionType,
    pub audio_features: Vec<AudioTrackFeature>,
    pub frame_metadata_features: Vec<PacketTrailerFeature>,
}

pub(crate) type MutedHandler = Box<dyn Fn(TrackPublication) + Send>;
pub(crate) type UnmutedHandler = Box<dyn Fn(TrackPublication) + Send>;

#[derive(Default)]
struct PublicationEvents {
    muted: Mutex<Option<MutedHandler>>,
    unmuted: Mutex<Option<UnmutedHandler>>,
}

pub(super) struct TrackPublicationInner {
    revision: Arc<StateRevision>,
    info: Observed<PublicationInfo>,
    events: Arc<PublicationEvents>,
}

/// Returns whether a `TrackInfo` represents a simulcasted publication.
///
/// `TrackInfo.simulcast` and `TrackInfo.layers` are deprecated and modern
/// LiveKit servers no longer populate them; the authoritative source is
/// `TrackInfo.codecs[*].layers`. We still consult the deprecated fields so
/// older servers continue to work.
fn is_simulcasted(info: &proto::TrackInfo) -> bool {
    info.simulcast || info.layers.len() > 1 || info.codecs.iter().any(|c| c.layers.len() > 1)
}

pub(super) fn new_inner(
    info: proto::TrackInfo,
    track: Option<Track>,
) -> Arc<TrackPublicationInner> {
    let revision = Arc::new(StateRevision::default());
    let info = PublicationInfo {
        desired: None,
        allowed: None,
        publish_options: None,
        track,
        simulcasted: is_simulcasted(&info),
        proto_info: info.clone(),
        source: info.source().into(),
        kind: info.r#type().try_into().unwrap(),
        encryption_type: info.encryption().into(),
        name: info.clone().name,
        sid: info.sid.clone().try_into().unwrap(),
        dimension: TrackDimension(info.width, info.height),
        mime_type: info.mime_type.clone(),
        muted: info.muted,
        audio_features: info
            .audio_features()
            .into_iter()
            .map(|item| item.try_into().unwrap())
            .collect(),
        frame_metadata_features: info
            .packet_trailer_features
            .iter()
            .filter_map(|v| PacketTrailerFeature::try_from(*v).ok())
            .collect(),
    };

    Arc::new(TrackPublicationInner {
        info: Observed::new(info, &revision),
        revision,
        events: Default::default(),
    })
}

pub(super) fn update_info(
    inner: &TrackPublicationInner,
    _publication: &TrackPublication,
    new_info: proto::TrackInfo,
) {
    let mut info = inner.info.write();
    if inner.revision.is_terminal() {
        return;
    }
    info.muted = new_info.muted;
    info.kind = TrackKind::try_from(new_info.r#type()).unwrap();
    info.source = TrackSource::from(new_info.source());
    info.encryption_type = new_info.encryption().into();
    info.proto_info = new_info.clone();
    info.name = new_info.name.clone();
    info.sid = new_info.sid.clone().try_into().unwrap();
    info.dimension = TrackDimension(new_info.width, new_info.height);
    info.mime_type = new_info.mime_type.clone();
    info.simulcasted = is_simulcasted(&new_info);
    info.audio_features = new_info.audio_features().collect();
    info.frame_metadata_features = new_info
        .packet_trailer_features
        .iter()
        .filter_map(|v| PacketTrailerFeature::try_from(*v).ok())
        .collect();
}

pub(super) fn set_track(
    inner: &TrackPublicationInner,
    publication: &TrackPublication,
    track: Option<Track>,
) {
    let mut info = inner.info.write();
    if inner.revision.is_terminal() {
        return;
    }
    let previous = info.track.clone();
    if let Some(prev_track) = previous.as_ref() {
        prev_track.on_muted(|_| {});
        prev_track.on_unmuted(|_| {});
    }

    info.track = track.clone();

    if let Some(track) = track.as_ref() {
        info.sid = track.sid();

        track.on_muted({
            let events = inner.events.clone();
            let publication = publication.clone();
            move |_| {
                if let Some(on_muted) = events.muted.lock().as_ref() {
                    on_muted(publication.clone());
                }
            }
        });

        track.on_unmuted({
            let events = inner.events.clone();
            let publication = publication.clone();
            move |_| {
                if let Some(on_unmuted) = events.unmuted.lock().as_ref() {
                    on_unmuted(publication.clone());
                }
            }
        });
    }
    drop(info);
    if let Some(previous) = previous {
        if !track.as_ref().is_some_and(|next| previous.same_instance(next)) {
            previous.finish_observation();
        }
    }
}

/// Immutable publication-owned values. Child mute/enabled values belong to track observation.
#[derive(Clone, Debug)]
pub struct PublicationValues {
    pub sid: TrackSid,
    pub name: String,
    pub kind: TrackKind,
    pub source: TrackSource,
    pub simulcasted: bool,
    pub dimension: TrackDimension,
    pub mime_type: String,
    pub muted: bool,
    pub encryption_type: EncryptionType,
    pub audio_features: Vec<AudioTrackFeature>,
    pub frame_metadata_features: Vec<PacketTrailerFeature>,
    pub desired: Option<bool>,
    pub allowed: Option<bool>,
    pub publish_options: Option<TrackPublishOptions>,
}

/// Values and live child membership; not deep-equatable.
#[derive(Clone, Debug)]
pub struct PublicationObservation {
    pub values: PublicationValues,
    pub track: Option<Track>,
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[tokio::test]
    async fn publication_mutations_are_owned_observed_and_terminal() {
        let remote = RemoteTrackPublication::new(
            proto::TrackInfo {
                sid: "TR_observed".into(),
                name: "before".into(),
                ..Default::default()
            },
            None,
            true,
        );
        let publication = TrackPublication::Remote(remote.clone());
        let initial = publication.observed_state();
        remote.set_subscribed(false);
        let desired = publication.state_did_change(Some(initial.revision)).await.unwrap();
        assert_eq!(initial.value.values.desired, Some(true));
        assert_eq!(desired.value.values.desired, Some(false));
        remote.update_info(proto::TrackInfo {
            sid: "TR_observed".into(),
            name: "after".into(),
            width: 640,
            height: 480,
            mime_type: "video/VP8".into(),
            muted: true,
            source: proto::TrackSource::Camera as i32,
            r#type: proto::TrackType::Video as i32,
            ..Default::default()
        });
        let changed = publication.state_did_change(Some(desired.revision)).await.unwrap();
        assert_eq!(changed.value.values.name, "after");
        assert_eq!(changed.value.values.dimension, TrackDimension(640, 480));
        assert_eq!(changed.value.values.source, TrackSource::Camera);
        assert_eq!(changed.value.values.kind, TrackKind::Video);
        assert_eq!(changed.value.values.mime_type, "video/VP8");
        assert!(changed.value.values.muted);
        assert_eq!(initial.value.values.name, "before");
        publication.finish_observation();
        let terminal = publication.state_did_change(Some(changed.revision)).await.unwrap();
        assert!(terminal.terminal);
        remote.set_subscribed(true);
        assert!(publication.state_did_change(Some(terminal.revision)).await.is_none());
        assert_eq!(publication.observed_state().value.values.desired, Some(false));
    }
}
