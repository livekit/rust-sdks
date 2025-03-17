use livekit::{Room, RoomOptions, StreamReader};
use std::{env, error::Error, sync::Arc};

const TOPIC: &str = "my-topic";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let is_sender = env::args()
        .nth(1)
        .map_or(false, |arg| arg == "sender");
    log::info!("Running as {}", if is_sender { "sender" } else { "receiver" });

    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await?;
    log::info!("Connected to room: {} - {}", room.name(), room.sid().await);

    tokio::select! {
        result = async {
            if is_sender {
                run_sender(&room).await
            } else {
                run_receiver(&room).await
            }
        } => result,
        _ = tokio::signal::ctrl_c() => {
            log::info!("Received Ctrl+C, shutting down");
            Ok(())
        }
    }
}

async fn run_sender(room: &Room) -> Result<(), Box<dyn Error>> {
    // TODO:
    Ok(())
}

async fn run_receiver(room: &Room) -> Result<(), Box<dyn Error>> {
    room.register_text_stream_handler(TOPIC.into(), |reader, identity| {
        Box::pin(async move {
            let full_message = reader.read_all().await?;
            log::info!("Message from {}: {}", identity, full_message);
            Ok(())
        })
    })?;
    Ok(())
}