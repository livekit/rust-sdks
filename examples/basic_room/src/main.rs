use livekit::prelude::*;
use std::env;

// Basic demo to connect to a room using the specified env variables

#[tokio::main]
async fn main() {
    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, mut rx) = Room::connect(&url, &token).await.unwrap();
    let session = room.session();
    println!("Connected to room: {} - {}", session.name(), session.sid());

    while let Some(msg) = rx.recv().await {
        println!("Event: {:?}", msg);
    }
}
