use futures_util::TryStreamExt;
use livekit::{
    e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        E2eeOptions, EncryptionType,
    },
    prelude::*,
    SimulateScenario, StreamReader, StreamTextOptions, TextStreamReader,
};
use livekit_api::access_token;
use std::{env, error::Error, io::Write};
use tokio::{
    io::{self, AsyncBufReadExt, BufReader},
    sync::mpsc::UnboundedReceiver,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".to_string());
    let api_key = env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".to_string());
    let api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".to_string());
    let room_name = env::var("LIVEKIT_ROOM").unwrap_or_else(|_| "dev".to_string());
    let identity = env::var("LIVEKIT_IDENTITY").unwrap_or_else(|_| "rust-participant".to_string());

    // Prompt for encryption password
    print!("Enter encryption password: ");
    std::io::stdout().flush()?;
    let mut password = String::new();
    std::io::stdin().read_line(&mut password)?;
    let password = password.trim().to_string();

    // Create access token
    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&identity)
        .with_name(&format!("{} (Encrypted)", identity))
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.clone(),
            ..Default::default()
        })
        .to_jwt()?;

    // Set up E2EE key provider
    let key_provider =
        KeyProvider::with_shared_key(KeyProviderOptions::default(), password.as_bytes().to_vec());

    // Configure room options with encryption
    let mut room_options = RoomOptions::default();
    room_options.encryption =
        Some(E2eeOptions { key_provider, encryption_type: livekit::e2ee::EncryptionType::Gcm });

    // Connect to room
    let (room, rx) = Room::connect(&url, &token, room_options).await?;
    println!("Connected to encrypted room: {} - {}", room.name(), room.sid().await);

    // Enable E2EE
    room.e2ee_manager().set_enabled(true);
    println!("End-to-end encryption enabled!");

    // Run the interactive chat
    run_interactive_chat(room, rx).await
}

async fn run_interactive_chat(
    room: Room,
    mut rx: UnboundedReceiver<RoomEvent>,
) -> Result<(), Box<dyn Error>> {
    println!("\n=== Encrypted Text Chat ===");
    println!("Type messages to send (press Enter). Type 'quit' to exit.");
    println!("Incoming messages will be displayed below:\n");

    let stdin = io::stdin();
    let mut stdin_reader = BufReader::new(stdin);

    loop {
        tokio::select! {
            // Handle user input
            input_result = read_user_input(&mut stdin_reader) => {
                match input_result {
                    Ok(Some(input)) => {
                        if input == "quit" {
                            println!("Goodbye!");
                            break;
                        }

                        // Send the message
                        let options = StreamTextOptions {
                            topic: "lk.chat".to_string(),
                            ..Default::default()
                        };

                        match room.local_participant().send_text(&input, options).await {
                            Ok(_) => {
                                println!("✓ Sent (encrypted): {}", input);
                            }
                            Err(e) => {
                                println!("✗ Failed to send: {}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        // Empty input, continue
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Error reading input: {}", e);
                        break;
                    }
                }
            }

            // Handle incoming room events
            event = rx.recv() => {
                match event {
                    Some(RoomEvent::TextStreamOpened { reader, topic, participant_identity }) => {
                        if topic == "lk.chat" {
                            if let Some(mut reader) = reader.take() {
                                match reader.read_all().await {
                                    Ok(message) => {
                                        println!("📨 {} (decrypted): {}", participant_identity, message);
                                    }
                                    Err(e) => {
                                        println!("✗ Failed to read message from {}: {}", participant_identity, e);
                                    }
                                }
                            }
                        }
                    }
                    Some(RoomEvent::ParticipantConnected( participant )) => {
                        println!("👋 {} joined the room", participant.identity());
                    }
                    Some(RoomEvent::ParticipantDisconnected ( participant )) => {
                        println!("👋 {} left the room", participant.identity());
                    }
                    Some(RoomEvent::Disconnected { reason }) => {
                        println!("Disconnected from room: {:?}", reason);
                        break;
                    }
                    Some(_) => {
                        // Ignore other events
                    }
                    None => {
                        println!("Room event stream ended");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn read_user_input(
    stdin_reader: &mut BufReader<io::Stdin>,
) -> Result<Option<String>, Box<dyn Error>> {
    let mut input = String::new();
    let bytes_read = stdin_reader.read_line(&mut input).await?;

    if bytes_read == 0 {
        return Ok(None); // EOF
    }

    let input = input.trim().to_string();
    if input.is_empty() {
        return Ok(None);
    }

    Ok(Some(input))
}
