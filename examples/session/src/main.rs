use std::env;
use std::error::Error;

use livekit_session::{
    Message, MessageOrigin, Session, SessionEvent, SessionOptions, SessionTokenSource,
};
use livekit_token_source::{development_token_server, literal, TokenSourceFetchOptions};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Reads credentials from a pre-generated token if one is given, and from a
/// sandbox token server otherwise.
fn token_source() -> Result<SessionTokenSource, Box<dyn Error>> {
    if let (Ok(url), Ok(token)) = (env::var("LIVEKIT_URL"), env::var("LIVEKIT_TOKEN")) {
        return Ok(literal(url, token).into());
    }
    let sandbox_id = env::var("LIVEKIT_SANDBOX_ID")
        .map_err(|_| "set LIVEKIT_SANDBOX_ID, or LIVEKIT_URL and LIVEKIT_TOKEN")?;
    let mut options = TokenSourceFetchOptions::new();
    if let Ok(agent_name) = env::var("LIVEKIT_AGENT_NAME") {
        options = options.with_agent_name(agent_name);
    }
    Ok((development_token_server(sandbox_id), options).into())
}

/// Prints a message once it is complete. The user's own input is skipped, as
/// it is already on screen.
fn print_message(message: &Message) {
    if !message.is_final {
        return;
    }
    let speaker = match message.origin {
        MessageOrigin::AgentTranscript => "agent",
        MessageOrigin::UserTranscript => "you (transcribed)",
        MessageOrigin::UserInput => return,
        _ => message.participant_identity.as_ref().map_or("chat", |p| p.as_str()),
    };
    println!("{speaker}: {}", message.text);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let session = Session::new(token_source()?, SessionOptions::default());
    let mut events = session.subscribe();
    session.start().await?;
    println!("Connected. Type a message and press enter; Ctrl-D ends the session.");

    let mut state = session.state();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            changed = state.changed() => {
                if changed.is_err() {
                    break;
                }
                let snapshot = *state.borrow_and_update();
                println!("[{:?}, agent {:?}]", snapshot.connection_state, snapshot.agent_state);
                if !snapshot.is_connected() {
                    break;
                }
            }
            Some(event) = events.recv() => {
                if let SessionEvent::MessageUpserted(message) = event {
                    print_message(&message);
                }
            }
            line = lines.next_line() => match line? {
                Some(line) if line.trim().is_empty() => {}
                Some(line) => {
                    if let Err(err) = session.send_text(line.trim()).await {
                        eprintln!("failed to send: {err}");
                    }
                }
                None => break,
            },
        }
    }

    // Fails only if the room already disconnected on its own.
    let _ = session.end().await;
    Ok(())
}
