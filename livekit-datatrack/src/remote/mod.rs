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

use crate::{DataTrack, DataTrackFrame, DataTrackInfo, DataTrackInner, InternalError};
use futures_util::{StreamExt, TryFutureExt};
use livekit_runtime::timeout;
use manager::{TrackState, TrackSubscriptionEvent};
use std::{marker::PhantomData, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, watch};
use tokio_stream::{wrappers::BroadcastStream, Stream};

mod manager;
mod pipeline;
mod proto;

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

impl DataTrack<Remote> {
    /// Subscribe to the data track to receive frames.
    pub async fn subscribe(&self) -> Result<impl Stream<Item = DataTrackFrame>, SubscribeError> {
        self.inner().subscribe().await
    }

    pub fn unsubscribe_all(self) {}

    /// Identity of the participant who published the track.
    pub fn publisher_identity(&self) -> &str {
        todo!()
    }

    // TODO: subscribe with options
    // TODO: is_published
}

#[derive(Debug)]
pub(crate) struct RemoteTrackInner {
    state_rx: watch::Receiver<TrackState>,
    subscription_tx: mpsc::Sender<TrackSubscriptionEvent>,
    frame_rx: broadcast::Receiver<DataTrackFrame>,
}

impl RemoteTrackInner {
    // manage subscription

    async fn subscribe(&self) -> Result<impl Stream<Item = DataTrackFrame>, SubscribeError> {
        self.require_subscription().await?;
        Ok(self.frame_stream())
    }

    async fn require_subscription(&self) -> Result<(), SubscribeError> {
        match *self.state_rx.borrow() {
            TrackState::Subscribed => return Ok(()),
            TrackState::Unpublished => return Err(SubscribeError::Unpublished),
            TrackState::Available => {}
        }
        self.subscription_tx
            .try_send(TrackSubscriptionEvent::Subscribe)
            .map_err(|e| Into::<anyhow::Error>::into(e))
            .map_err(InternalError::from)?;

        let mut state_rx = self.state_rx.clone();
        let wait_for_subscribed = state_rx
            .wait_for(|state| matches!(state, TrackState::Subscribed))
            .map_err(|_| SubscribeError::Disconnected);

        _ = timeout(Duration::from_secs(10), wait_for_subscribed)
            .map_err(|_| SubscribeError::Timeout)
            .await??;
        Ok(())
    }

    fn frame_stream(&self) -> impl Stream<Item = DataTrackFrame> {
        // TODO: mechanism to end stream on unsubscribe but not unpublish
        BroadcastStream::new(self.frame_rx.resubscribe())
            .filter_map(|result| async move { result.ok() })
    }
}

impl Drop for RemoteTrackInner {
    fn drop(&mut self) {
        // unsubscribe
    }
}

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("The track has been unpublished and is no longer available")]
    Unpublished,
    #[error("Request to subscribe to data track timed-out")]
    Timeout,
    #[error("Cannot subscribe to data track when disconnected")]
    Disconnected,
    #[error(transparent)]
    Internal(#[from] InternalError),
}
