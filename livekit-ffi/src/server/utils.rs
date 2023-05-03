use crate::{server, FFIError, FFIResult};
use livekit::prelude::*;

pub fn find_remote_track(
    server: &'static server::FFIServer,
    track_sid: &TrackSid,
    participant_sid: &ParticipantSid,
    room_sid: &RoomSid,
) -> FFIResult<RemoteTrack> {
    let session = server
        .rooms
        .read()
        .get(room_sid)
        .ok_or(FFIError::InvalidRequest("room not found"))?
        .session();

    let participant = session
        .participants()
        .get(participant_sid)
        .ok_or(FFIError::InvalidRequest("participant not found"))?;

    let track = participant
        .get_track_publication(track_sid)
        .ok_or(FFIError::InvalidRequest("publication not found"))?
        .track()
        .ok_or(FFIError::InvalidRequest("track not found/subscribed"))?;

    Ok(track)
}
