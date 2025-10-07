#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::Result,
    chrono::{TimeDelta, TimeZone, Utc},
    common::test_rooms,
    livekit::{ConnectionState, ParticipantKind},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_connect() -> Result<()> {
    let (room, _) = test_rooms(1).await?.pop().unwrap();

    assert_eq!(room.connection_state(), ConnectionState::Connected);
    assert!(room.name().starts_with("test_room_"));
    assert!(room.remote_participants().is_empty());

    let creation_time = Utc.timestamp_opt(room.creation_time(), 0).unwrap();
    assert!(creation_time.signed_duration_since(Utc::now()).abs() <= TimeDelta::seconds(10));

    let local_participant = room.local_participant();
    assert!(local_participant.sid().as_str().starts_with("PA_"));
    assert_eq!(local_participant.identity().as_str(), "p0");
    assert_eq!(local_participant.name(), "Participant 0");
    assert_eq!(local_participant.kind(), ParticipantKind::Standard);

    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_connect_multiple() -> Result<()> {
    let mut rooms = test_rooms(2).await?;

    let (second_room, _) = rooms.pop().unwrap();
    let (first_room, _) = rooms.pop().unwrap();

    assert_eq!(first_room.name(), second_room.name(), "Participants are in different rooms");

    assert!(second_room
        .remote_participants()
        .get(&first_room.local_participant().identity())
        .is_some());
    assert!(first_room
        .remote_participants()
        .get(&second_room.local_participant().identity())
        .is_some());

    Ok(())
}
