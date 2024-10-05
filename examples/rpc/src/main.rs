use livekit::prelude::*;
use livekit_api::access_token;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let room_name = format!("rpc-test-{:x}", rand::thread_rng().gen::<u32>());
    println!("Connecting participants to room: {}", room_name);

    let (requesters_room, requesters_rx) = connect_participant("requester", &room_name, &url, &api_key, &api_secret).await?;
    let (greeters_room, greeters_rx) = connect_participant("greeter", &room_name, &url, &api_key, &api_secret).await?;
    let (math_genius_room, math_genius_rx) = connect_participant("math-genius", &room_name, &url, &api_key, &api_secret).await?;

    register_receiver_methods(&greeters_room, &math_genius_room).await;

    println!("\n\nRunning greeting example...");
    perform_greeting(&requesters_room).await?;

    println!("\n\nRunning math example...");
    perform_square_root(&requesters_room).await?;
    sleep(Duration::from_secs(2)).await;
    perform_quantum_hypergeometric_series(&requesters_room).await?;

    println!("\n\nParticipants done, disconnecting...");
    requesters_room.disconnect().await?;
    greeters_room.disconnect().await?;
    math_genius_room.disconnect().await?;

    println!("Participants disconnected. Example completed.");

    Ok(())
}

async fn register_receiver_methods(greeters_room: &Room, math_genius_room: &Room) {
    greeters_room.local_participant().register_rpc_method("arrival", |sender, _, payload, _| {
        Box::pin(async move {
            println!("[Greeter] Oh {} arrived and said \"{}\"", sender.identity(), payload);
            sleep(Duration::from_secs(2)).await;
            Ok("Welcome and have a wonderful day!".to_string())
        })
    });

    math_genius_room.local_participant().register_rpc_method("square-root", |sender, _, payload, response_timeout_ms| {
        Box::pin(async move {
            let json_data: Value = serde_json::from_str(&payload).unwrap();
            let number = json_data["number"].as_f64().unwrap();
            println!(
                "[Math Genius] I guess {} wants the square root of {}. I've only got {} seconds to respond but I think I can pull it off.",
                sender.identity(),
                number,
                response_timeout_ms / 1000
            );

            println!("[Math Genius] *doing math*…");
            sleep(Duration::from_secs(2)).await;

            let result = number.sqrt();
            println!("[Math Genius] Aha! It's {}", result);
            Ok(json!({"result": result}).to_string())
        })
    });
}

async fn perform_greeting(room: &Room) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Requester] Letting the greeter know that I've arrived");
    match room.local_participant().perform_rpc_request("greeter", "arrival", "Hello".to_string(), None).await {
        Ok(response) => println!("[Requester] That's nice, the greeter said: \"{}\"", response),
        Err(e) => println!("[Requester] RPC call failed: {:?}", e),
    }
    Ok(())
}

async fn perform_square_root(room: &Room) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Requester] What's the square root of 16?");
    match room.local_participant().perform_rpc_request("math-genius", "square-root", json!({"number": 16}).to_string(), None).await {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[Requester] Nice, the answer was {}", parsed_response["result"]);
        },
        Err(e) => println!("[Requester] RPC call failed: {:?}", e),
    }
    Ok(())
}

async fn perform_quantum_hypergeometric_series(room: &Room) -> Result<(), Box<dyn std::error::Error>> {
    println!("[Requester] What's the quantum hypergeometric series of 42?");
    match room.local_participant().perform_rpc_request("math-genius", "quantum-hypergeometric-series", json!({"number": 42}).to_string(), None).await {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[Requester] genius says {}!", parsed_response["result"]);
        },
        Err(e) => {
            if let Some(rpc_error) = e.downcast_ref::<RpcError>() {
                if rpc_error.code() == RpcErrorCode::UnsupportedMethod {
                    println!("[Requester] Aww looks like the genius doesn't know that one.");
                    return Ok(());
                }
            }
            println!("[Requester] Unexpected error: {:?}", e);
        },
    }
    Ok(())
}

async fn connect_participant(identity: &str, room_name: &str, url: &str, api_key: &str, api_secret: &str) -> Result<(Room, tokio::sync::mpsc::Receiver<RoomEvent>), Box<dyn std::error::Error>> {
    let token = access_token::AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            ..Default::default()
        })
        .to_jwt()?;

    let (room, rx) = Room::connect(url, &token, RoomOptions::default()).await?;

    tokio::spawn({
        let identity = identity.to_string();
        async move {
            while let Some(event) = rx.recv().await {
                if let RoomEvent::Disconnected { .. } = event {
                    println!("[{}] Disconnected from room", identity);
                    break;
                }
            }
        }
    });

    Ok((room, rx))
}
