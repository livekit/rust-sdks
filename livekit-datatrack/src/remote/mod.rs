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

use crate::api::{DataTrack, DataTrackFrame, DataTrackInfo, DataTrackInner, InternalError};
use anyhow::anyhow;
use futures_util::StreamExt;
use livekit_runtime::timeout;
use manager::SubscribeEvent;
use std::{marker::PhantomData, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_stream::{wrappers::BroadcastStream, Stream};

pub(crate) mod manager;
pub(crate) mod proto;

mod depacketizer;
mod pipeline;

/// Data track published by a remote participant.
pub type RemoteDataTrack = DataTrack<Remote>;

/// Marker type indicating a [`DataTrack`] belongs to a remote participant.
///
/// See also: [`RemoteDataTrack`]
///
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
    /// Subscribes to the data track to receive frames.
    ///
    /// # Returns
    ///
    /// A stream that yields [`DataTrackFrame`]s as they arrive.
    ///
    /// # Multiple Subscriptions
    ///
    /// An application may call `subscribe` more than once to process frames in
    /// multiple places. For example, one async task might plot values on a graph
    /// while another writes them to a file.
    ///
    /// Internally, only the first call to `subscribe` communicates with the SFU and
    /// allocates the resources required to receive frames. Additional subscriptions
    /// reuse the same underlying pipeline and do not trigger additional signaling.
    ///
    /// Note that newly created subscriptions only receive frames published after
    /// the initial subscription is established.
    ///
    pub async fn subscribe(&self) -> Result<impl Stream<Item = DataTrackFrame>, SubscribeError> {
        let (result_tx, result_rx) = oneshot::channel();
        let subscribe_event = SubscribeEvent { sid: self.info.sid.clone(), result_tx };
        self.inner()
            .event_in_tx
            .upgrade()
            .ok_or(SubscribeError::Disconnected)?
            .send_timeout(subscribe_event.into(), Duration::from_millis(50))
            .await
            .map_err(|_| {
                SubscribeError::Internal(anyhow!("Failed to send subscribe event").into())
            })?;

        // TODO: standardize timeout
        let frame_rx = timeout(Duration::from_secs(10), result_rx)
            .await
            .map_err(|_| SubscribeError::Timeout)?
            .map_err(|_| SubscribeError::Disconnected)??;

        let frame_stream =
            BroadcastStream::new(frame_rx).filter_map(|result| async move { result.ok() });
        Ok(Box::pin(frame_stream))
    }

    /// Identity of the participant who published the track.
    pub fn publisher_identity(&self) -> &str {
        &self.inner().publisher_identity
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteTrackInner {
    publisher_identity: Arc<str>,
    published_rx: watch::Receiver<bool>,
    event_in_tx: mpsc::WeakSender<manager::InputEvent>,
}

impl RemoteTrackInner {
    pub fn published_rx(&self) -> watch::Receiver<bool> {
        self.published_rx.clone()
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
