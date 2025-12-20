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

use from_variants::FromVariants;
use std::{marker::PhantomData, sync::Arc};

pub use crate::dtp::TrackHandle;

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

#[derive(Debug, Clone)]
pub struct DataTrack<L> {
    pub(crate) info: Arc<DataTrackInfo>,
    pub(crate) inner: DataTrackInner,
    /// Marker indicating local or remote.
    pub(crate) _location: PhantomData<L>,
}

#[derive(Debug, Clone, FromVariants)]
pub(crate) enum DataTrackInner {
    Local(crate::local::manager::LocalDataTrackInner),
    Remote(()), // TODO: add sub inner
}

impl<L> DataTrack<L> {
    /// Information about the data track such as name.
    pub fn info(&self) -> &DataTrackInfo {
        &self.info
    }
}
