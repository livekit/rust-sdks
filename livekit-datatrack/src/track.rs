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

use crate::{
    error::PublishFrameError,
    frame::DataTrackFrame,
    manager::{self},
};
use from_variants::FromVariants;
use std::{marker::PhantomData, sync::Arc};

pub use crate::dtp::TrackHandle;

/// Options for publishing a data track.
#[derive(Clone, Debug)]
pub struct PublishOptions {
    pub(crate) name: String,
    pub(crate) disable_e2ee: bool,
}

impl PublishOptions {
    pub fn with_name(name: impl Into<String>) -> Self {
        Self { name: name.into(), disable_e2ee: false }
    }
    pub fn disable_e2ee(self, disabled: bool) -> Self {
        Self { disable_e2ee: disabled, ..self }
    }
}

#[derive(Debug, Clone)]
pub struct DataTrackInfo {
    pub(crate) sid: String, // TODO: use shared ID type
    pub(crate) handle: TrackHandle,
    pub(crate) name: String,
    pub(crate) uses_e2ee: bool,
}

impl DataTrackInfo {
    pub fn sid(&self) -> &String {
        &self.sid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn uses_e2ee(&self) -> bool {
        self.uses_e2ee
    }
}

/// Marker type indicating a [`DataTrack`] belongs to the local participant.
#[derive(Debug)]
pub struct Local;

/// Marker type indicating a [`DataTrack`] belongs to a remote participant.
#[derive(Debug)]
pub struct Remote;

#[derive(Debug, Clone)]
pub struct DataTrack<L> {
    info: Arc<DataTrackInfo>,
    inner: DataTrackInner,
    /// Marker indicating local or remote.
    _location: PhantomData<L>,
}

#[derive(Debug, Clone, FromVariants)]
enum DataTrackInner {
    Local(manager::PubHandle),
    Remote(()), // TODO: add sub handle
}

impl<L> DataTrack<L> {
    /// Information about the data track such as name.
    pub fn info(&self) -> &DataTrackInfo {
        &self.info
    }
}

impl DataTrack<Local> {
    pub(crate) fn new(info: Arc<DataTrackInfo>, handle: manager::PubHandle) -> Self {
        Self { info, inner: handle.into(), _location: PhantomData }
    }

    fn handle(&self) -> &manager::PubHandle {
        match &self.inner {
            DataTrackInner::Local(publisher) => publisher,
            DataTrackInner::Remote(_) => unreachable!(), // Safe (type state)
        }
    }

    /// Publish a frame onto the track.
    pub fn publish(&self, frame: impl Into<DataTrackFrame>) -> Result<(), PublishFrameError> {
        Ok(self.handle().publish(frame.into())?)
    }

    /// Whether or not the track is still published.
    pub fn is_published(&self) -> bool {
        self.handle().is_published()
    }

    /// Unpublish the track.
    pub fn unpublish(self) {
        self.handle().unpublish()
    }
}

// TODO: implement remote track (subscriber)