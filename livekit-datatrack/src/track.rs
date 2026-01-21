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

use crate::packet::Handle;
use from_variants::FromVariants;
use std::{fmt::Display, marker::PhantomData, ops::Deref, sync::Arc};
use tokio::sync::watch;
use thiserror::Error;

/// Track for communicating application-specific data between participants in room.
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
    /// Information about the data track.
    pub fn info(&self) -> &DataTrackInfo {
        &self.info
    }

    /// Whether or not the track is still published.
    pub fn is_published(&self) -> bool {
        let published_rx = self.published_rx();
        let published = *published_rx.borrow();
        published
    }

    /// Waits asynchronously until the track is unpublished.
    ///
    /// Use this to trigger follow-up work once the track is no longer published.
    /// If the track is already unpublished, this method returns immediately.
    ///
    pub async fn wait_for_unpublish(&self) {
        let mut published_rx = self.published_rx();
        if !*published_rx.borrow() {
            // Already unpublished
            return;
        }
        _ = published_rx.wait_for(|is_published| !*is_published).await;
    }

    fn published_rx(&self) -> watch::Receiver<bool> {
        match self.inner.as_ref() {
            DataTrackInner::Local(inner) => inner.published_rx(),
            DataTrackInner::Remote(inner) => inner.published_rx(),
        }
    }
}

/// Information about a published data track.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct DataTrackInfo {
    pub(crate) sid: DataTrackSid,
    pub(crate) pub_handle: Handle,
    pub(crate) name: String,
    pub(crate) uses_e2ee: bool,
}

impl DataTrackInfo {
    /// Unique track identifier.
    pub fn sid(&self) -> &DataTrackSid {
        &self.sid
    }
    /// Name of the track assigned by the publisher.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Whether or not frames sent on the track use end-to-end encryption.
    pub fn uses_e2ee(&self) -> bool {
        self.uses_e2ee
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

#[cfg(test)]
impl fake::Dummy<fake::Faker> for DataTrackSid {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        const BASE_57_ALPHABET: &[u8; 57] =
            b"23456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let random_id: String = (0..12)
            .map(|_| {
                let idx = rng.random_range(0..BASE_57_ALPHABET.len());
                BASE_57_ALPHABET[idx] as char
            })
            .collect();
        Self::try_from(format!("{}{}", Self::PREFIX, random_id)).unwrap()
    }
}
