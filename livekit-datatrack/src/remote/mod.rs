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

use crate::{DataTrack, DataTrackInfo, DataTrackInner};
use std::{marker::PhantomData, sync::Arc};

mod manager;
mod proto;
mod pipeline;

pub(crate) use pipeline::RemoteTrackInner;

/// Data track published by a remote participant.
pub type RemoteDataTrack = DataTrack<Remote>;

/// Marker type indicating a [`DataTrack`] belongs to a remote participant.
#[derive(Debug, Clone)]
pub struct Remote;

impl DataTrack<Remote> {
    pub(crate) fn new(info: Arc<DataTrackInfo>, inner: RemoteTrackInner) -> Self {
        Self { info, inner: Arc::new(inner.into()), _location: PhantomData }
    }

    fn inner(&self) -> &RemoteTrackInner {
        match &*self.inner {
            DataTrackInner::Remote(inner) => inner,
            DataTrackInner::Local(_) => unreachable!(), // Safe (type state)
        }
    }
}

impl DataTrack<Remote> {}
