use anyhow::Result;
use livekit::{
    data_track::{DataTrack, DataTrackFrameBuilder, DataTrackOptions, Local},
    prelude::*,
};
use std::{
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{signal, time};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, rx) = Room::connect(&url, &token, RoomOptions::default()).await?;

    let options = DataTrackOptions::with_name("brightness");
    let track = room.local_participant().publish_data_track(options).await?;

    tokio::select! {
        _ = publish_frames(track) => {}
        _ = signal::ctrl_c() => {}
    }
    Ok(())
}

async fn publish_frames(track: DataTrack<Local>) {
    loop {
        let frame = DataTrackFrameBuilder::new(vec![0xFA; 256]);
        track
            .publish(frame.build())
            .inspect_err(|err| println!("Failed to publish frame: {}", err))
            .ok();
        time::sleep(Duration::from_millis(500)).await
    }
}
