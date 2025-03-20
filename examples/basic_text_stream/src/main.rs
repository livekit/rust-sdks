use futures_util::TryStreamExt;
use livekit::{Room, RoomOptions, StreamTextOptions, StreamWriter};
use std::{env, error::Error, time::Duration};
use tokio::time::sleep;

static TOPIC: &str = "my-topic";
static WORDS: &[&str] = &["This", "text", "will", "be", "sent", "incrementally."];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let is_sender = env::args().nth(1).map_or(false, |arg| arg == "sender");

    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await?;
    println!("Connected to room: {} - {}", room.name(), room.sid().await);

    if is_sender {
        run_sender(&room).await
    } else {
        run_receiver(&room).await
    }
}

async fn run_sender(room: &Room) -> Result<(), Box<dyn Error>> {
    println!("Running as sender");
    loop {
        let options = StreamTextOptions { topic: TOPIC.to_string(), ..Default::default() };
        let writer = room.local_participant().stream_text(options).await?;
        println!("Opened new stream");

        for word in WORDS.iter() {
            writer.write(*word).await?;
            println!("Sent '{}'", word);
            sleep(Duration::from_millis(500)).await;
        }
        writer.close().await?;
        println!("Stream complete");
    }
}

async fn run_receiver(room: &Room) -> Result<(), Box<dyn Error>> {
    println!("Running as receiver");
    println!("Waiting for incoming streams…");
    room.register_text_stream_handler(TOPIC, |mut reader, identity| {
        println!("New stream from {}", identity);
        Box::pin(async move {
            while let Some((chunk, _)) = reader.try_next().await? {
                println!("Chunk received from {}: '{}'", identity, chunk);
            }
            Ok(())
        })
    })?;
    Ok(tokio::signal::ctrl_c().await?)
}
