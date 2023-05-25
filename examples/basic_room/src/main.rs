use livekit::prelude::*;
use std::env;

// Connect to a room using the specified env variables
// and print all incoming events

#[tokio::main]
async fn main() {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, mut rx) = Room::connect(&url, &token).await.unwrap();
    log::info!("Connected to room: {} - {}", room.name(), room.sid());

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }
}
