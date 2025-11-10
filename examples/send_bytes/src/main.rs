use livekit::{Room, RoomEvent, RoomOptions, StreamByteOptions, StreamReader};
use packet::LedControlPacket;
use rand::Rng;
use std::{env, error::Error, time::Duration};
use tokio::{sync::mpsc::UnboundedReceiver, time::sleep};

mod packet;

const LED_CONTROL_TOPIC: &str = "led-control";

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
    let mut rng = rand::rng();

    loop {
        // Send control packets with randomized channel and color.
        let packet = packet::LedControlPacket::new()
            .with_version(1)
            .with_channel(rng.random_range(0..16))
            .with_is_on(true)
            .with_red(rng.random())
            .with_green(rng.random())
            .with_blue(rng.random());

        println!("[tx] {}", packet);

        let options = StreamByteOptions { topic: LED_CONTROL_TOPIC.into(), ..Default::default() };
        let be_bytes = packet.into_bits().to_be_bytes();
        room.local_participant().send_bytes(&be_bytes, options).await?;

        sleep(Duration::from_millis(500)).await;
    }
}

async fn run_receiver(
    _room: Room,
    mut rx: UnboundedReceiver<RoomEvent>,
) -> Result<(), Box<dyn Error>> {
    println!("Running as receiver");
    println!("Waiting for LED control packets…");
    while let Some(event) = rx.recv().await {
        match event {
            RoomEvent::ByteStreamOpened { reader, topic, participant_identity: _ } => {
                if topic != LED_CONTROL_TOPIC {
                    continue;
                };
                let Some(reader) = reader.take() else { continue };

                let Ok(be_bytes) = reader.read_all().await?[..4].try_into() else {
                    log::warn!("Unexpected packet length");
                    continue;
                };
                let packet = LedControlPacket::from(u32::from_be_bytes(be_bytes));

                println!("[rx] {}", packet);
            }
            _ => {}
        }
    }
    Ok(())
}
