use anyhow::Result;
use livekit::prelude::*;
use std::{env, time::Duration};
use tokio::{signal, time};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await?;

    let track = room.local_participant().publish_data_track("my_sensor_data").await?;

    tokio::select! {
        _ = push_frames(track) => {}
        _ = signal::ctrl_c() => {}
    }
    Ok(())
}

async fn read_sensor() -> Vec<u8> {
    // Dynamically read some sensor data...
    vec![0xFA; 256]
}

async fn push_frames(track: LocalDataTrack) {
    loop {
        log::info!("Pushing frame");

        let reading = read_sensor().await;
        let frame = DataTrackFrame::new(reading).with_user_timestamp_now();

        track.try_push(frame).inspect_err(|err| println!("Failed to push frame: {}", err)).ok();
        time::sleep(Duration::from_millis(500)).await
    }
}
