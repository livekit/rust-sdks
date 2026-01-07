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

use crate::dtp::Handle;
use from_variants::FromVariants;
use std::{fmt::Display, marker::PhantomData, ops::Deref, sync::Arc};
use thiserror::Error;

/// Information about a published data track.
#[derive(Debug, Clone)]
pub struct DataTrackInfo {
    pub(crate) sid: DataTrackSid,
    pub(crate) handle: Handle, // TODO: consider removing (protocol level detail)
    pub(crate) name: String,
    pub(crate) uses_e2ee: bool,
}

impl DataTrackInfo {
    /// Unique track identifier.
    pub fn sid(&self) -> &DataTrackSid {
        &self.sid
    }
    /// Name of the track assigned when published.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Whether or not frames sent on the track use end-to-end encryption.
    pub fn uses_e2ee(&self) -> bool {
        self.uses_e2ee
    }
}

#[derive(Debug, Clone)]
pub struct DataTrack<L> {
    pub(crate) info: Arc<DataTrackInfo>,
    pub(crate) inner: Arc<DataTrackInner>,
    /// Marker indicating local or remote.
    pub(crate) _location: PhantomData<L>,
}

#[derive(Debug, Clone, FromVariants)]
pub(crate) enum DataTrackInner {
    Local(crate::local::LocalTrackInner),
    Remote(crate::remote::RemoteTrackInner),
}

impl<L> DataTrack<L> {
    /// Information about the data track such as name.
    pub fn info(&self) -> &DataTrackInfo {
        &self.info
    }
}

/// SFU-assigned identifier uniquely identifying a data track.
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct DataTrackSid(String);

#[derive(Debug, Error)]
#[error("Invalid data track SID")]
pub struct DataTrackSidError;

impl DataTrackSid {
    const PREFIX: &str = "DTR_";
}

impl TryFrom<String> for DataTrackSid {
    type Error = DataTrackSidError;

    fn try_from(raw_id: String) -> Result<Self, Self::Error> {
        if raw_id.starts_with(Self::PREFIX) {
            Ok(Self(raw_id))
        } else {
            Err(DataTrackSidError)
        }
    }
}

impl From<DataTrackSid> for String {
    fn from(id: DataTrackSid) -> Self {
        id.0
    }
}

impl Deref for DataTrackSid {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for DataTrackSid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
