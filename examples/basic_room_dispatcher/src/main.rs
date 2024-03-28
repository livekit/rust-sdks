use livekit::prelude::*;
use livekit_api::access_token;
use std::env;
use std::time::Duration;

struct SmolDispatcher;

impl livekit::dispatcher::Dispatcher for SmolDispatcher {
    fn dispatch(&self, runnable: livekit::dispatcher::Runnable) {
        smol::spawn(async {
            runnable.run();
        })
        .detach();
    }

    fn dispatch_after(&self, duration: Duration, runnable: livekit::dispatcher::Runnable) {
        let timer = smol::Timer::after(duration);
        smol::spawn(async {
            timer.await;
            runnable.run();
        })
        .detach();
    }
}

// Connect to a room using the specified env variables
// and print all incoming events
fn main() {
    livekit::dispatcher::set_dispatcher(SmolDispatcher {});

    smol::block_on(async {
        env_logger::init();

        let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
        let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
        let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

        let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
            .with_identity("rust-bot")
            .with_name("Rust Bot")
            .with_grants(access_token::VideoGrants {
                room_join: true,
                room: "my-room".to_string(),
                ..Default::default()
            })
            .to_jwt()
            .unwrap();

        let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
        log::info!("Connected to room: {} - {}", room.name(), room.sid());

        room.local_participant()
            .publish_data(DataPacket {
                payload: "Hello world".to_owned().into_bytes(),
                kind: DataPacketKind::Reliable,
                ..Default::default()
            })
            .await
            .unwrap();

        while let Some(msg) = rx.recv().await {
            log::info!("Event: {:?}", msg);
        }
    })
}
