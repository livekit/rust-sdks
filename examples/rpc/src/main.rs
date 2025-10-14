use livekit::prelude::*;
use livekit_api::access_token;
use rand::Rng;
use serde_json::{json, Value};
use std::env;
use std::sync::Once;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;

// Example usage of RPC calls between participants
// (In a real app, you'd have one participant per client/device such as an agent and a browser app)
//
// Try it with `LIVEKIT_URL=<url> LIVEKIT_API_KEY=<your-key> LIVEKIT_API_SECRET=<your-secret> cargo run`

static START_TIME: Once = Once::new();
static mut START_INSTANT: Option<Instant> = None;

fn get_start_time() -> Instant {
    unsafe {
        START_TIME.call_once(|| {
            START_INSTANT = Some(Instant::now());
        });
        START_INSTANT.unwrap()
    }
}

fn elapsed_time() -> String {
    let start = get_start_time();
    let elapsed = Instant::now().duration_since(start);
    format!("+{:.3}s", elapsed.as_secs_f64())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Initialize START_TIME
    get_start_time();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let room_name = format!("rpc-test-{:x}", rand::thread_rng().gen::<u32>());
    println!("[{}] Connecting participants to room: {}", elapsed_time(), room_name);

    let (callers_room, greeters_room, math_genius_room) = tokio::try_join!(
        connect_participant("caller", &room_name, &url, &api_key, &api_secret),
        connect_participant("greeter", &room_name, &url, &api_key, &api_secret),
        connect_participant("math-genius", &room_name, &url, &api_key, &api_secret)
    )?;

    register_receiver_methods(greeters_room.clone(), math_genius_room.clone()).await;

    println!("\n\nRunning greeting example...");
    perform_greeting(&callers_room).await?;

    println!("\n\nRunning error handling example...");
    perform_division(&callers_room).await?;

    println!("\n\nRunning math example...");
    perform_square_root(&callers_room).await?;
    sleep(Duration::from_secs(2)).await;
    perform_quantum_hypergeometric_series(&callers_room).await?;

    println!("\n\nRunning nested calculation example...");
    perform_nested_calculation(&callers_room).await?;

    println!("\n\nParticipants done, disconnecting...");
    callers_room.close().await?;
    greeters_room.close().await?;
    math_genius_room.close().await?;

    println!("Participants disconnected. Example completed.");

    Ok(())
}

async fn register_receiver_methods(greeters_room: Arc<Room>, math_genius_room: Arc<Room>) {
    greeters_room.local_participant().register_rpc_method("arrival".to_string(), |data| {
        Box::pin(async move {
            println!(
                "[{}] [Greeter] Oh {} arrived and said \"{}\"",
                elapsed_time(),
                data.caller_identity,
                data.payload
            );
            sleep(Duration::from_secs(2)).await;
            Ok("Welcome and have a wonderful day!".to_string())
        })
    });

    math_genius_room.local_participant().register_rpc_method(
        "square-root".to_string(),
        |data| {
            Box::pin(async move {
                let json_data: Value = serde_json::from_str(&data.payload).unwrap();
                let number = json_data["number"].as_f64().unwrap();
                println!(
                    "[{}] [Math Genius] I guess {} wants the square root of {}. I've only got {} seconds to respond but I think I can pull it off.",
                    elapsed_time(),
                    data.caller_identity,
                    number,
                    data.response_timeout.as_secs()
                );

                println!("[{}] [Math Genius] *doing math*…", elapsed_time());
                sleep(Duration::from_secs(2)).await;

                let result = number.sqrt();
                println!("[{}] [Math Genius] Aha! It's {}", elapsed_time(), result);
                Ok(json!({"result": result}).to_string())
            })
        },
    );

    math_genius_room.local_participant().register_rpc_method("divide".to_string(), |data| {
        Box::pin(async move {
            let json_data: Value = serde_json::from_str(&data.payload).unwrap();
            let dividend = json_data["dividend"].as_i64().unwrap();
            let divisor = json_data["divisor"].as_i64().unwrap();
            println!(
                "[{}] [Math Genius] {} wants me to divide {} by {}.",
                elapsed_time(),
                data.caller_identity,
                dividend,
                divisor
            );

            let result = dividend / divisor;
            println!("[{}] [Math Genius] The result is {}", elapsed_time(), result);
            Ok(json!({"result": result}).to_string())
        })
    });

    math_genius_room.local_participant().register_rpc_method(
        "nested-calculation".to_string(),
        move |data| {
            let math_genius_room = math_genius_room.clone();
            Box::pin(async move {
                let json_data: Value = serde_json::from_str(&data.payload).unwrap();
                let number = json_data["number"].as_f64().unwrap();
                println!(
                    "[{}] [Math Genius] {} wants me to do a nested calculation on {}.",
                    elapsed_time(),
                    data.caller_identity,
                    number
                );

                match math_genius_room
                    .local_participant()
                    .perform_rpc(PerformRpcData {
                        destination_identity: data.caller_identity.to_string(),
                        method: "provide-intermediate".to_string(),
                        payload: json!({"original": number}).to_string(),
                        ..Default::default()
                    })
                    .await
                {
                    Ok(intermediate_response) => {
                        let intermediate: Value =
                            serde_json::from_str(&intermediate_response).unwrap();
                        let intermediate_value = intermediate["value"].as_f64().unwrap();
                        let final_result = intermediate_value * 2.0;
                        println!(
                            "[{}] [Math Genius] Got intermediate value {}, final result is {}",
                            elapsed_time(),
                            intermediate_value,
                            final_result
                        );
                        Ok(json!({"result": final_result}).to_string())
                    }
                    Err(e) => Err(RpcError {
                        code: 1,
                        message: "Failed to get intermediate result".to_string(),
                        data: None,
                    }),
                }
            })
        },
    );
}

async fn perform_greeting(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] Letting the greeter know that I've arrived", elapsed_time());
    match room
        .local_participant()
        .perform_rpc(PerformRpcData {
            destination_identity: "greeter".to_string(),
            method: "arrival".to_string(),
            payload: "Hello".to_string(),
            ..Default::default()
        })
        .await
    {
        Ok(response) => {
            println!("[{}] That's nice, the greeter said: \"{}\"", elapsed_time(), response)
        }
        Err(e) => println!("[{}] RPC call failed: {:?}", elapsed_time(), e),
    }
    Ok(())
}

async fn perform_square_root(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] What's the square root of 16?", elapsed_time());
    match room
        .local_participant()
        .perform_rpc(PerformRpcData {
            destination_identity: "math-genius".to_string(),
            method: "square-root".to_string(),
            payload: json!({"number": 16}).to_string(),
            ..Default::default()
        })
        .await
    {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[{}] Nice, the answer was {}", elapsed_time(), parsed_response["result"]);
        }
        Err(e) => log::error!("[{}] RPC call failed: {:?}", elapsed_time(), e),
    }
    Ok(())
}

async fn perform_quantum_hypergeometric_series(
    room: &Arc<Room>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] What's the quantum hypergeometric series of 42?", elapsed_time());
    match room
        .local_participant()
        .perform_rpc(PerformRpcData {
            destination_identity: "math-genius".to_string(),
            method: "quantum-hypergeometric-series".to_string(),
            payload: json!({"number": 42}).to_string(),
            ..Default::default()
        })
        .await
    {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[{}] genius says {}!", elapsed_time(), parsed_response["result"]);
        }
        Err(e) => {
            if e.code == RpcErrorCode::UnsupportedMethod as u32 {
                println!("[{}] Aww looks like the genius doesn't know that one.", elapsed_time());
                return Ok(());
            }
            log::error!("[{}] RPC error: {} (code: {})", elapsed_time(), e.message, e.code);
        }
    }
    Ok(())
}

async fn perform_division(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{}] Let's try dividing 5 by 0", elapsed_time());
    match room
        .local_participant()
        .perform_rpc(PerformRpcData {
            destination_identity: "math-genius".to_string(),
            method: "divide".to_string(),
            payload: json!({"dividend": 5, "divisor": 0}).to_string(),
            ..Default::default()
        })
        .await
    {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[{}] The result is {}", elapsed_time(), parsed_response["result"]);
        }
        Err(e) => {
            println!("[{}] Oops! Dividing by zero didn't work. That's ok...", elapsed_time());
            log::error!("[{}] RPC error: {} (code: {})", elapsed_time(), e.message, e.code);
        }
    }

    Ok(())
}

async fn perform_nested_calculation(room: &Arc<Room>) -> Result<(), Box<dyn std::error::Error>> {
    room.local_participant().register_rpc_method("provide-intermediate".to_string(), |data| {
        Box::pin(async move {
            let json_data: Value = serde_json::from_str(&data.payload).unwrap();
            let original = json_data["original"].as_f64().unwrap();
            let intermediate = original + 10.0;
            println!(
                "[{}] [Caller] Providing intermediate calculation: {} + 10 = {}",
                elapsed_time(),
                original,
                intermediate
            );
            Ok(json!({"value": intermediate}).to_string())
        })
    });

    println!("[{}] Starting nested calculation with value 5", elapsed_time());
    match room
        .local_participant()
        .perform_rpc(PerformRpcData {
            destination_identity: "math-genius".to_string(),
            method: "nested-calculation".to_string(),
            payload: json!({"number": 5.0}).to_string(),
            ..Default::default()
        })
        .await
    {
        Ok(response) => {
            let parsed_response: Value = serde_json::from_str(&response)?;
            println!("[{}] Final result: {}", elapsed_time(), parsed_response["result"]);
        }
        Err(e) => log::error!("[{}] RPC call failed: {:?}", elapsed_time(), e),
    }
    Ok(())
}

async fn connect_participant(
    identity: &str,
    room_name: &str,
    url: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<Arc<Room>, Box<dyn std::error::Error>> {
    let token = access_token::AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            ..Default::default()
        })
        .to_jwt()?;

    println!("[{}] [{}] Connecting...", elapsed_time(), identity);
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
