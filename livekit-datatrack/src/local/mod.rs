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
///
/// See also: [`LocalDataTrack`]
///
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
    /// Try pushing a frame to subscribers of the track.
    ///
    /// # Example
    ///
    /// ```
    /// # use livekit_datatrack::api::{LocalDataTrack, DataTrackFrame, PushFrameError};
    /// # fn example(track: LocalDataTrack) -> Result<(), PushFrameError> {
    /// fn read_sensor() -> Vec<u8> {
    ///     // Read some sensor data...
    ///     vec![0xFA; 16]
    /// }
    ///
    /// let frame = read_sensor().into(); // Convert to frame
    /// track.try_push(frame)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// See [`DataTrackFrame`] for more ways to construct a frame and how to attach metadata.
    ///
    /// # Errors
    ///
    /// Pushing a frame can fail for several reasons:
    ///
    /// - The track has been unpublished by the local participant or SFU
    /// - The room is no longer connected
    /// - Frames are being pushed too fast
    ///
    pub fn try_push(&self, frame: DataTrackFrame) -> Result<(), PushFrameError> {
        if !self.is_published() {
            return Err(PushFrameError::new(frame, PushFrameErrorReason::TrackUnpublished));
        }
        self.inner().frame_tx.try_send(frame).map_err(|err| {
            PushFrameError::new(err.into_inner(), PushFrameErrorReason::Dropped)
        })
    }

    /// Unpublishes the track.
    pub fn unpublish(self) {
        self.inner().local_unpublish();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalTrackInner {
    pub frame_tx: mpsc::Sender<DataTrackFrame>,
    pub published_tx: watch::Sender<bool>,
}

impl LocalTrackInner {
    fn local_unpublish(&self) {
        _ = self.published_tx.send(false);
    }

    pub fn published_rx(&self) -> watch::Receiver<bool> {
        self.published_tx.subscribe()
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
/// Create options for publishing a track named "my_track":
///
/// ```
/// # use livekit_datatrack::api::DataTrackOptions;
/// let options = DataTrackOptions::new("my_track");
/// ```
///
#[derive(Clone, Debug)]
pub struct DataTrackOptions {
    pub(crate) name: String,
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
        Self { name: name.into() }
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
pub struct PushFrameError {
    frame: DataTrackFrame,
    reason: PushFrameErrorReason,
}

impl PushFrameError {
    pub(crate) fn new(frame: DataTrackFrame, reason: PushFrameErrorReason) -> Self {
        Self { frame, reason }
    }

    /// Returns the reason the frame could not be pushed.
    pub fn reason(&self) -> PushFrameErrorReason {
        self.reason
    }

    /// Consumes the error and returns the frame that couldn't be pushed.
    ///
    /// This may be useful for implementing application-specific retry logic.
    ///
    pub fn into_frame(self) -> DataTrackFrame {
        self.frame
    }
}

/// Reason why a data track frame could not be pushed.
#[derive(Debug, Clone, Copy)]
pub enum PushFrameErrorReason {
    /// Track is no longer published.
    TrackUnpublished,
    /// Frame was dropped.
    Dropped,
}

impl fmt::Display for PushFrameErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackUnpublished => write!(f, "track unpublished"),
            Self::Dropped => write!(f, "dropped"),
        }
    }
}
