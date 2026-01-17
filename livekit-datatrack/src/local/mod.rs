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
    api::{DataTrack, DataTrackFrame, DataTrackInfo, InternalError},
    track::DataTrackInner,
};
use manager::{LocalTrackState, UnpublishInitiator};
use std::{fmt, marker::PhantomData, sync::Arc};
use thiserror::Error;
use tokio::sync::{mpsc, watch};

pub(crate) mod manager;
pub(crate) mod proto;

mod packetizer;
mod pipeline;

/// Data track published by the local participant.
pub type LocalDataTrack = DataTrack<Local>;

/// Marker type indicating a [`DataTrack`] belongs to the local participant.
#[derive(Debug)]
pub struct Local;

impl DataTrack<Local> {
    pub(crate) fn new(info: Arc<DataTrackInfo>, inner: LocalTrackInner) -> Self {
        Self { info, inner: Arc::new(inner.into()), _location: PhantomData }
    }

    fn inner(&self) -> &LocalTrackInner {
        match &*self.inner {
            DataTrackInner::Local(track) => track,
            DataTrackInner::Remote(_) => unreachable!(), // Safe (type state)
        }
    }
}

impl DataTrack<Local> {
    /// Publishes a frame to the track.
    ///
    /// # Example
    ///
    /// ```
    /// # use livekit_datatrack::api::{LocalDataTrack, DataTrackFrame, PublishFrameError};
    /// # fn example(track: LocalDataTrack) -> Result<(), PublishFrameError> {
    /// fn read_sensor() -> Vec<u8> {
    ///     // Read some sensor data...
    ///     vec![0xFA; 16]
    /// }
    ///
    /// let frame = read_sensor().into(); // Convert to frame
    /// track.publish(frame)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// See [`DataTrackFrame`] for more ways to construct a frame and how to attach metadata.
    ///
    /// # Errors
    ///
    /// Publishing a frame can fail for several reasons:
    ///
    /// - The track has been unpublished by the local participant or SFU
    /// - The room is no longer connected
    /// - Frames are being published too fast
    ///
    pub fn publish(&self, frame: DataTrackFrame) -> Result<(), PublishFrameError> {
        if !self.is_published() {
            return Err(PublishFrameError::new(frame, PublishFrameErrorReason::TrackUnpublished));
        }
        self.inner().frame_tx.try_send(frame).map_err(|err| {
            PublishFrameError::new(err.into_inner(), PublishFrameErrorReason::Dropped)
        })
    }

    /// Whether or not the track is still published.
    ///
    /// Once the track has been unpublished, calls to [`Self::publish`] will fail.
    ///
    pub fn is_published(&self) -> bool {
        self.inner().state_tx.borrow().is_published()
    }

    /// Unpublishes the track.
    pub fn unpublish(self) {
        self.inner().local_unpublish();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalTrackInner {
    pub frame_tx: mpsc::Sender<DataTrackFrame>,
    pub state_tx: watch::Sender<LocalTrackState>,
}

impl LocalTrackInner {
    fn local_unpublish(&self) {
        self.state_tx
            .send(LocalTrackState::Unpublished { initiator: UnpublishInitiator::Client })
            .inspect_err(|err| log::error!("Failed to update state to unsubscribed: {err}"))
            .ok();
    }
}

impl Drop for LocalTrackInner {
    fn drop(&mut self) {
        // Implicit unpublish when handle dropped.
        self.local_unpublish();
    }
}

/// Options for publishing a data track.
///
/// # Examples
///
/// Create options for publishing a track named "my_track" with end-to-end encryption disabled:
/// ```
/// # use livekit_datatrack::api::DataTrackOptions;
/// let options = DataTrackOptions::new("my_track")
///     .disable_e2ee(true); // Set additional options as needed
/// ```
///
#[derive(Clone, Debug)]
pub struct DataTrackOptions {
    pub(crate) name: String,
    pub(crate) disable_e2ee: bool,
}

impl DataTrackOptions {
    /// Creates options with the given track name.
    ///
    /// The track name is used to identify the track to other participants.
    ///
    /// # Requirements
    /// - Must not be empty
    /// - Must be unique per publisher
    ///
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), disable_e2ee: false }
    }

    /// Disable end-to-end encryption.
    ///
    /// By default, room settings are used.
    ///
    pub fn disable_e2ee(mut self, disabled: bool) -> Self {
        self.disable_e2ee = disabled;
        self
    }
}

impl From<String> for DataTrackOptions {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl From<&str> for DataTrackOptions {
    fn from(name: &str) -> Self {
        Self::new(name.to_string())
    }
}

/// An error that can occur when publishing a data track.
#[derive(Debug, Error)]
pub enum PublishError {
    /// Local participant does not have permission to publish data tracks.
    ///
    /// Ensure the participant's token contains the `canPublishData` grant.
    ///
    #[error("Data track publishing unauthorized")]
    NotAllowed,

    /// A track with the same name is already published by the local participant.
    #[error("Track name already taken")]
    DuplicateName,

    /// Request to publish the track took long to complete.
    #[error("Publish data track timed-out")]
    Timeout,

    /// No additional data tracks can be published by the local participant.
    #[error("Data track publication limit reached")]
    LimitReached,

    /// Cannot publish data track when the room is disconnected.
    #[error("Room disconnected")]
    Disconnected,

    /// Internal error, please report on GitHub.
    #[error(transparent)]
    Internal(#[from] InternalError),
}

/// Frame could not be published to a data track.
#[derive(Debug, Error)]
#[error("Failed to publish frame: {reason}")]
pub struct PublishFrameError {
    frame: DataTrackFrame,
    reason: PublishFrameErrorReason,
}

impl PublishFrameError {
    pub(crate) fn new(frame: DataTrackFrame, reason: PublishFrameErrorReason) -> Self {
        Self { frame, reason }
    }

    /// Returns the reason the frame could not be published.
    pub fn reason(&self) -> PublishFrameErrorReason {
        self.reason
    }

    /// Consumes the error and returns the frame that couldn't be published.
    ///
    /// This may be useful for implementing application-specific retry logic.
    ///
    pub fn into_frame(self) -> DataTrackFrame {
        self.frame
    }
}

/// Reason why a data track frame could not be published.
#[derive(Debug, Clone, Copy)]
pub enum PublishFrameErrorReason {
    /// Track is no longer published.
    TrackUnpublished,
    /// Frame was dropped.
    Dropped,
}
// TODO: could provide unpublish reason and more
// info about why the frame was dropped.

impl fmt::Display for PublishFrameErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackUnpublished => write!(f, "track unpublished"),
            Self::Dropped => write!(f, "dropped"),
        }
    }
}
