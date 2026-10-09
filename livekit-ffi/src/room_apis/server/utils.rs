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

use livekit::prelude::{RoomEvent, Track, TrackSource};
use tokio::sync::{broadcast, mpsc};

use super::participant::FfiParticipant;
use crate::{server, FfiError, FfiHandleId};

pub async fn track_changed_trigger(
    participant: FfiParticipant,
    track_source: TrackSource,
    track_tx: mpsc::Sender<Track>,
    track_finished_tx: broadcast::Sender<Track>,
) {
    for track_pub in participant.participant.track_publications().values() {
        if track_pub.source() == track_source.into() {
            if let Some(track) = track_pub.track() {
                let _ = track_tx.send(track).await;
            }
        }
    }
    let room = &participant.room.room;
    let mut room_event_rx = room.subscribe();
    while let Some(event) = room_event_rx.recv().await {
        match event {
            RoomEvent::TrackSubscribed { track, publication, participant: p } => {
                if participant.participant.identity() != p.identity() {
                    continue;
                }
                if publication.source() == track_source.into() {
                    let _ = track_tx.send(track.into()).await;
                }
            }
            RoomEvent::TrackUnsubscribed { track, publication, participant: p } => {
                if p.identity() != participant.participant.identity() {
                    continue;
                }
                if publication.source() == track_source.into() {
                    let _ = track_finished_tx.send(track.into());
                }
            }
            RoomEvent::ParticipantDisconnected(p) => {
                if p.identity() == participant.participant.identity() {
                    return;
                }
            }
            RoomEvent::Disconnected { reason: _ } => {
                break;
            }
            _ => {}
        }
    }
}

pub fn ffi_participant_from_handle(
    server: &'static server::FfiServer,
    handle_id: FfiHandleId,
) -> Result<FfiParticipant, FfiError> {
    let ffi_participant_handle = server.retrieve_handle::<FfiParticipant>(handle_id);
    if ffi_participant_handle.is_err() {
        return Err(FfiError::InvalidRequest("participant not found".into()));
    }
    return Ok(ffi_participant_handle.unwrap().clone());
}
