use futures::StreamExt;
use livekit::prelude::*;
use livekit_api::access_token;
use std::env;

// Connect to a room using the specified env variables
// and read data streams

#[tokio::main]
async fn main() {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "dev".to_string(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap();

    log::info!("Connecting to room");
    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    log::info!("Connected to room: {}", room.name());

    room.local_participant()
        .publish_data(DataPacket {
            payload: "Hello world".to_owned().into_bytes(),
            reliable: true,
            ..Default::default()
        })
        .await
        .unwrap();

    while let Some(msg) = rx.recv().await {
        match msg {
            RoomEvent::TextStreamReceived { mut stream_reader } => {
                log::info!("TextStreamReceived: {:?}", stream_reader.info.stream_id);

                tokio::spawn(async move {
                    let mut collected = String::new();
                    while let Some(chunk) = stream_reader.next().await {
                        log::info!("received text frame - {:?}", chunk.current);
                        collected = chunk.collected;
                    }
                    log::info!("finished reading text stream: {:?}", collected);
                });
            }
            RoomEvent::FileStreamReceived { mut stream_reader } => {
                log::info!("FileStreamReceived: {:?}", stream_reader.info.stream_id);

                tokio::spawn(async move {
                    let file_name = stream_reader.info.file_name.clone();
                    let mut data: Vec<u8> = vec![];
                    while let Some(mut chunk) = stream_reader.next().await {
                        data.append(&mut chunk);
                    }
                    log::info!("finished reading file stream, now writing to disk");
                    std::fs::write(file_name, data).unwrap();
                });
            }
            other => {
                log::debug!("Event: {:?}", other);
            }
        }
    }
}
