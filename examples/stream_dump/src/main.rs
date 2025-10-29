use futures_util::TryStreamExt;
use livekit::{Room, RoomEvent, RoomOptions};
use std::{env, error::Error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await?;
    println!("Connected to room: {} - {}", room.name(), room.sid().await);

    while let Some(event) = rx.recv().await {
        let RoomEvent::ByteStreamOpened {
            reader,
            topic,
            participant_identity,
        } = event
        else {
            continue;
        };
        let Some(mut reader) = reader.take() else {
            continue;
        };
        println!(
            "Byte stream opened: topic={}, participant={}",
            topic,
            participant_identity
        );
        while let Some(chunk) = reader.try_next().await? {
            println!("Chunk: {} bytes", chunk.len());
        }
    }
    Ok(())
}
