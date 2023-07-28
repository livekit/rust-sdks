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

use super::TrackPublicationInner;
use crate::prelude::*;
use livekit_protocol as proto;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Clone)]
pub struct LocalTrackPublication {
    inner: Arc<TrackPublicationInner>,
}

impl Debug for LocalTrackPublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalTrackPublication")
            .field("sid", &self.sid())
            .field("name", &self.name())
            .field("kind", &self.kind())
            .finish()
    }
}

impl LocalTrackPublication {
    pub(crate) fn new(info: proto::TrackInfo, track: LocalTrack) -> Self {
        Self {
            inner: super::new_inner(info, Some(track.into())),
        }
    }

    pub(crate) fn on_muted(&self, f: impl Fn(TrackPublication, Track) + Send + 'static) {
        *self.inner.events.muted.lock() = Some(Box::new(f));
    }

    pub(crate) fn on_unmuted(&self, f: impl Fn(TrackPublication, Track) + Send + 'static) {
        *self.inner.events.unmuted.lock() = Some(Box::new(f));
    }

    pub(crate) fn set_track(&self, track: Option<Track>) {
        super::set_track(&self.inner, &TrackPublication::Local(self.clone()), track);
    }

    #[allow(dead_code)]
    pub(crate) fn update_info(&self, info: proto::TrackInfo) {
        super::update_info(&self.inner, &TrackPublication::Local(self.clone()), info);
    }

    pub fn mute(&self) {
        self.track().mute();
    }

    pub fn unmute(&self) {
        self.track().unmute();
    }

    pub fn sid(&self) -> String {
        self.inner.info.read().sid.clone()
    }

    pub fn name(&self) -> String {
        self.inner.info.read().name.clone()
    }

    pub fn kind(&self) -> TrackKind {
        self.inner.info.read().kind
    }

    pub fn source(&self) -> TrackSource {
        self.inner.info.read().source
    }

    pub fn simulcasted(&self) -> bool {
        self.inner.info.read().simulcasted
    }

    pub fn dimension(&self) -> TrackDimension {
        self.inner.info.read().dimension
    }

    pub fn track(&self) -> LocalTrack {
        self.inner
            .info
            .read()
            .track
            .clone()
            .unwrap()
            .try_into()
            .unwrap()
    }

    pub fn mime_type(&self) -> String {
        self.inner.info.read().mime_type.clone()
    }

    pub fn is_muted(&self) -> bool {
        self.inner.info.read().muted
    }

    pub fn is_remote(&self) -> bool {
        false
    }
}
