use crate::{server, FfiError, FfiResult};
use livekit::prelude::*;

pub fn find_remote_track(
    server: &'static server::FfiServer,
    track_sid: &TrackSid,
    participant_sid: &ParticipantSid,
    room_sid: &RoomSid,
) -> FfiResult<RemoteTrack> {
    let session = server
        .rooms
        .read()
        .get(room_sid)
        .ok_or(FfiError::InvalidRequest("room not found"))?
        .session();

    let participants = session.participants();
    let participant = participants
        .get(participant_sid)
        .ok_or(FfiError::InvalidRequest("participant not found"))?;

    let track = participant
        .get_track_publication(track_sid)
        .ok_or(FfiError::InvalidRequest("publication not found"))?
        .track()
        .ok_or(FfiError::InvalidRequest("track not found/subscribed"))?;

    Ok(track)
}
