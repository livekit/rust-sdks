use livekit::prelude::*;
use std::env;

// Connect to a room using the specified env variables
// and print all incoming events

#[tokio::main]
async fn main() {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .unwrap();
    log::info!("Connected to room: {} - {}", room.name(), room.sid());

    room.local_participant()
        .publish_data(
            "Hello world".to_owned().into_bytes(),
            DataPacketKind::Reliable,
            Default::default(),
        )
        .await
        .unwrap();

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }
}
