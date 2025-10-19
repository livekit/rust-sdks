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

use crate::{frame::DataTrackFrame, mime::Mime};
use futures_util::{task::Context, Stream};
use std::{marker::PhantomData, pin::Pin, task::Poll};

/// Options for publishing a data track.
#[derive(Clone, Debug)]
pub struct PublishOptions {
    pub(crate) name: String,
    pub(crate) disable_e2ee: bool,
    pub(crate) mime: Mime,
}

impl PublishOptions {
    pub fn with_name(name: impl Into<String>) -> Self {
        Self { name: name.into(), disable_e2ee: false, mime: Mime::BINARY }
    }

    pub fn mime(self, mime: Mime) -> Self {
        Self { mime, ..self }
    }

    pub fn disable_e2ee(self, disabled: bool) -> Self {
        Self { disable_e2ee: disabled, ..self }
    }
}

#[derive(Clone, Debug)]
struct DataTrackInfo {
    sid: String, // TODO: use shared ID type
    handle: u16,
    name: String,
    mime: Mime,
    uses_e2ee: bool,
}

impl DataTrackInfo {
    pub fn sid(&self) -> &String {
        &self.sid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mime(&self) -> &Mime {
        &self.mime
    }
    pub fn uses_e2ee(&self) -> bool {
        self.uses_e2ee
    }
}

/// Marker type indicating a [`DataTrack`] belongs to the local participant.
pub struct Local;

/// Marker type indicating a [`DataTrack`] belongs to a remote participant.
pub struct Remote;

#[derive(Clone, Debug)]
pub struct DataTrack<L> {
    /// Marker indicating local or remote.
    _location: PhantomData<L>,
    // Need info, way to signal closing by SFU or other

    // Cases:
    // Local (publish) -> channel tx
    // Remote (subscribe) -> channel rx
}

impl<L> DataTrack<L> {
    pub fn info(&self) -> DataTrackInfo {
        todo!()
    }
}

impl DataTrack<Local> {
    pub fn publish(&self, frame: impl Into<DataTrackFrame>) -> DataTrackResult<()> {
        todo!()
    }
}

impl DataTrack<Remote> {
    pub(crate) fn from_info(info: DataTrackInfo) -> Result<Self, ()> {
        Ok(Self { _location: PhantomData })
    }

    pub fn is_subscribed() -> bool {
        // Subscribed as long as there is at least one subscription
        todo!()
    }

    pub fn subscribe(&self) -> DataTrackResult<DataTrackSubscription> {
        // TODO: send request, create receiver
        todo!()
    }

    pub fn subscribe_with_target(&self, target_fps: u32) -> DataTrackResult<DataTrackSubscription> {
        todo!()
    }
}

pub struct DataTrackSubscription;

impl Stream for DataTrackSubscription {
    type Item = DataTrackFrame;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        todo!();
    }
}
