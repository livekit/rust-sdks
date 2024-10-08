use livekit::prelude::*;
use livekit_api::access_token;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;
use serde_json::{json, Value};
use std::sync::Arc;

// Example usage of RPC calls between participants
// (In a real app, you'd have one participant per client/device such as an agent and a browser app)
//
// Try it with `LIVEKIT_URL=<url> LIVEKIT_API_KEY=<your-key> LIVEKIT_API_SECRET=<your-secret> cargo run`

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let room_name = format!("rpc-test-{:x}", rand::thread_rng().gen::<u32>());
    println!("Connecting participants to room: {}", room_name);

    let callers_room = connect_participant("caller", &room_name, &url, &api_key, &api_secret).await?;
    let greeters_room = connect_participant("greeter", &room_name, &url, &api_key, &api_secret).await?;
    let math_genius_room = connect_participant("math-genius", &room_name, &url, &api_key, &api_secret).await?;

    register_receiver_methods(&greeters_room, &math_genius_room).await;

    println!("\n\nRunning greeting example...");
    perform_greeting(&callers_room).await?;

    println!("\n\nRunning math example...");
    perform_square_root(&callers_room).await?;
    sleep(Duration::from_secs(2)).await;
    perform_quantum_hypergeometric_series(&callers_room).await?;

    println!("\n\nParticipants done, disconnecting...");
    callers_room.close().await?;
    greeters_room.close().await?;
    math_genius_room.close().await?;

    println!("Participants disconnected. Example completed.");

    Ok(())
}

async fn register_receiver_methods(greeters_room: &Arc<Room>, math_genius_room: &Arc<Room>) {
    greeters_room.local_participant().register_rpc_method("arrival".to_string(), |_, caller_identity, payload, _| {
        Box::pin(async move {
            println!("[Greeter] Oh {} arrived and said \"{}\"", caller_identity, payload);
            sleep(Duration::from_secs(2)).await;
            Ok("Welcome and have a wonderful day!".to_string())
        })
    });

    math_genius_room.local_participant().register_rpc_method("square-root".to_string(), |_, caller_identity, payload, response_timeout_ms| {
        Box::pin(async move {
            let json_data: Value = serde_json::from_str(&payload).unwrap();
            let number = json_data["number"].as_f64().unwrap();
            println!(
                "[Math Genius] I guess {} wants the square root of {}. I've only got {} seconds to respond but I think I can pull it off.",
                caller_identity,
                number,
                response_timeout_ms.as_secs()
            );

            println!("[Math Genius] *doing math*…");
            sleep(Duration::from_secs(2)).await;

            let result = number.sqrt();
            println!("[Math Genius] Aha! It's {}", result);
            Ok(json!({"result": result}).to_string())
        })
    });
}

async fn perform_greeting(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Caller] Letting the greeter know that I've arrived");
    match room.local_participant().perform_rpc("greeter".to_string(), "arrival".to_string(), "Hello".to_string(), None).await {
        Ok(response) => println!("[Caller] That's nice, the greeter said: \"{}\"", response),
        Err(e) => println!("[Caller] RPC call failed: {:?}", e),
    }
    Ok(())
}

async fn perform_square_root(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Caller] What's the square root of 16?");
    match room.local_participant().perform_rpc("math-genius".to_string(), "square-root".to_string(), json!({"number": 16}).to_string(), None).await {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[Caller] Nice, the answer was {}", parsed_response["result"]);
        },
        Err(e) => log::error!("[Caller] RPC call failed: {:?}", e),
    }
    Ok(())
}

async fn perform_quantum_hypergeometric_series(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Caller] What's the quantum hypergeometric series of 42?");
    match room.local_participant().perform_rpc("math-genius".to_string(), "quantum-hypergeometric-series".to_string(), json!({"number": 42}).to_string(), None).await {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[Caller] genius says {}!", parsed_response["result"]);
        },
        Err(e) => {
            if e.code == RpcErrorCode::UnsupportedMethod as u32 {
                println!("[Caller] Aww looks like the genius doesn't know that one.");
                return Ok(());
            }
            log::error!("[Caller] RPC error: {} (code: {})", e.message, e.code);
        },
    }
    Ok(())
}

async fn connect_participant(identity: &str, room_name: &str, url: &str, api_key: &str, api_secret: &str) -> Result<Arc<Room>, Box<dyn std::error::Error>> {
    let token = access_token::AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            ..Default::default()
        })
        .to_jwt()?;

    let (room, mut rx) = Room::connect(url, &token, RoomOptions::default()).await?;

    let room = Arc::new(room);

    tokio::spawn({
        let identity = identity.to_string();
        let room_clone = Arc::clone(&room);
        async move {
            while let Some(event) = rx.recv().await {
                if let RoomEvent::Disconnected { .. } = event {
                    println!("[{}] Disconnected from room", identity);
                    break;
                }
            }
            room_clone.close().await.ok();
        }
    });

    Ok(room)
}
