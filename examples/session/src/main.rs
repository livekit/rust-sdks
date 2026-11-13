use std::error::Error;
use std::{env, time::Duration};

use livekit_session::{
    Message, MessageOrigin, Session, SessionEvent, SessionOptions, SessionState,
};

use livekit_token_source::{development_token_server, literal, TokenSourceFetchOptions};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Creates a session with credentials from a pre-generated token if one is
/// given, and from a development token server otherwise.
fn create_session(options: SessionOptions) -> Result<Session, Box<dyn Error>> {
    if let (Ok(url), Ok(token)) = (env::var("LIVEKIT_URL"), env::var("LIVEKIT_TOKEN")) {
        return Ok(Session::new(literal(url, token), options));
    }
    let token_server_id = env::var("LIVEKIT_TOKEN_SERVER_ID")
        .map_err(|_| "set LIVEKIT_TOKEN_SERVER_ID, or LIVEKIT_URL and LIVEKIT_TOKEN")?;
    let mut fetch_options = TokenSourceFetchOptions::new();
    if let Ok(agent_name) = env::var("LIVEKIT_AGENT_NAME") {
        fetch_options = fetch_options.with_agent_name(agent_name);
    }
    Ok(Session::with_fetch_options(
        development_token_server(token_server_id),
        fetch_options,
        options,
    ))
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
        _ => message.participant_identity.as_str(),
    };
    println!("{speaker}: {}", message.text);
}

fn print_state(state: &SessionState) {
    println!("[{:?}, agent {:?}]", state.connection, state.agent);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // By default the session captures the microphone and plays the agent's
    // audio, with echo cancellation between the two. `--text` keeps the
    // conversation to typed lines, e.g. on a machine without audio devices.
    let text_only = env::args().any(|arg| arg == "--text");
    let session_options = SessionOptions::default()
        .with_agent_connect_timeout(Duration::from_secs(15))
        .with_platform_audio(!text_only);
    let session = create_session(session_options)?;
    let mut events = session.subscribe();
    session.start().await?;

    // `changed` reports only later updates, so print where the session starts.
    let mut state = session.state();
    print_state(&state.borrow_and_update());
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        tokio::select! {
            changed = state.changed() => {
                if changed.is_err() {
                    break;
                }
                let current = *state.borrow_and_update();
                print_state(&current);
                if !current.connection.is_connected() {
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

    session.end().await;
    Ok(())
}
