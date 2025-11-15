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

use crate::frame::DataTrackFrame;
use core::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("The local participant does not have permission to publish data tracks")]
    NotAllowed,
    #[error("A data track with the same name is already published by the local participant")]
    DuplicateName,
    #[error("Request to publish data track timed-out")]
    Timeout,
    #[error("No more data tracks are able to be published")]
    LimitReached,
    #[error(transparent)]
    Internal(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum SubscribeError {}

/// An error that can occur when publishing a frame onto a data track.
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

    /// Consume the error, returning the frame that couldn't be published.
    pub fn into_frame(self) -> DataTrackFrame {
        self.frame
    }

    /// Returns the reason why the frame could not be published.
    pub fn reason(&self) -> PublishFrameErrorReason {
        self.reason
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

#[derive(Debug, Error)]
#[error(transparent)]
pub struct InternalError(#[from] anyhow::Error);
