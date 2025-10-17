#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{Ok, Result},
    common::test_rooms,
    livekit::data_track::{schema, Mime},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_data_track() -> Result<()> {
    use livekit::data_track::DataTrackOptions;

    let (room, mut event_rx) = test_rooms(1).await?.pop().unwrap();

    let options = DataTrackOptions::with_name("led_color")
        .mime(Mime::JSON)
        .disable_e2ee(false);

    let track = room.local_participant().publish_data_track(options).await?;
    for idx in 1..25 {
        // track.publish()
    }

    while let Some(event) = event_rx.recv().await {
        use livekit::RoomEvent;
        let RoomEvent::TrackPublished { publication, participant } = event else { continue };

    }


    Ok(())
}
