use livekit::prelude::{RoomEvent, Track, TrackSource};
use tokio::sync::mpsc;

use super::room::FfiParticipant;

pub async fn track_changed_trigger(
    participant: FfiParticipant,
    track_source: TrackSource,
    track_tx: mpsc::Sender<Track>,
) {
    for track_pub in participant.participant.track_publications().values() {
        if track_pub.source() == track_source.into() {
            let track = track_pub.track();
            match track {
                Some(track) => {
                    track_tx.send(track).await;
                }
                _ => {}
            }
        }
    }
    let room = &participant.room.room;
    let mut room_event_rx = room.subscribe();
    while let Some(event) = room_event_rx.recv().await {
        match event {
            RoomEvent::TrackPublished { publication, participant: p } => {
                if participant.participant.identity() != p.identity() {
                    continue;
                }
                if publication.source() == track_source.into() {
                    let track = publication.track();
                    match track {
                        Some(track) => {
                            track_tx.send(track.into()).await;
                        }
                        _ => {}
                    }
                }
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                if participant.identity() == participant.identity() {
                    break;
                }
            }
            RoomEvent::Disconnected { reason: _ } => {
                break;
            }
            _ => {}
        }
    }
}
