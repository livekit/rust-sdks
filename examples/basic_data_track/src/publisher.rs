use anyhow::Result;
use livekit::prelude::*;
use std::{env, time::Duration};
use tokio::{signal, time};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");
    let reliability = data_track_reliability();

    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await?;

    let track = room
        .local_participant()
        .publish_data_track(DataTrackOptions::new("my_sensor_data").reliability(reliability))
        .await?;

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
        log::info!("Sending frame");

        let reading = read_sensor().await;
        let frame = DataTrackFrame::new(reading).with_user_timestamp_now();

        track
            .send_frame(frame)
            .await
            .inspect_err(|err| println!("Failed to send frame: {}", err))
            .ok();
        time::sleep(Duration::from_millis(500)).await
    }
}

fn data_track_reliability() -> DataTrackReliability {
    match env::var("DATA_TRACK_RELIABILITY").as_deref() {
        Ok("reliable") => DataTrackReliability::Reliable,
        Ok("lossy") | Err(_) => DataTrackReliability::Lossy,
        Ok(value) => {
            panic!("Unsupported DATA_TRACK_RELIABILITY '{value}', expected lossy or reliable")
        }
    }
}
