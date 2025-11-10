use futures_util::TryStreamExt;
use livekit::{Room, RoomEvent, RoomOptions, StreamTextOptions, StreamWriter};
use std::{env, error::Error, time::Duration};
use tokio::{sync::mpsc::UnboundedReceiver, time::sleep};

static TOPIC: &str = "my-topic";
static WORDS: &[&str] = &["This", "text", "will", "be", "sent", "incrementally."];

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let is_sender = env::args().nth(1).map_or(false, |arg| arg == "sender");

    let (room, rx) = Room::connect(&url, &token, RoomOptions::default()).await?;
    println!("Connected to room: {} - {}", room.name(), room.sid().await);

    if is_sender {
        run_sender(room).await
    } else {
        run_receiver(room, rx).await
    }
}

async fn run_sender(room: Room) -> Result<(), Box<dyn Error>> {
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

async fn run_receiver(
    _room: Room,
    mut rx: UnboundedReceiver<RoomEvent>,
) -> Result<(), Box<dyn Error>> {
    println!("Running as receiver");
    println!("Waiting for incoming streams…");
    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
        match msg {
            RoomEvent::TextStreamOpened { reader, topic, participant_identity } => {
                if topic != TOPIC {
                    continue;
                };
                let Some(mut reader) = reader.take() else { continue };
                while let Some(chunk) = reader.try_next().await? {
                    println!("Chunk received from {}: '{}'", participant_identity, chunk);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
