use std::error::Error;
use std::time::Duration;

use clap::Parser;
use livekit_session::{
    Message, MessageOrigin, Session, SessionEvent, SessionOptions, SessionState,
};

use livekit_token_source::{development_token_server, literal, TokenSourceFetchOptions};
use tokio::io::{AsyncBufReadExt, BufReader};

/// A conversation with a LiveKit agent over voice and typed text
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit server URL, used together with --token
    #[arg(long, env = "LIVEKIT_URL")]
    url: Option<String>,

    /// Pre-generated token whose room configuration dispatches the agent
    #[arg(long, env = "LIVEKIT_TOKEN", hide_env_values = true)]
    token: Option<String>,

    /// Development token server ID, used when no URL and token are given
    #[arg(long, env = "LIVEKIT_TOKEN_SERVER_ID")]
    token_server_id: Option<String>,

    /// Agent for the development token server to dispatch
    #[arg(long, env = "LIVEKIT_AGENT_NAME")]
    agent_name: Option<String>,

    /// Leave the audio devices alone and talk by typing only
    #[arg(long)]
    text: bool,
}

/// Creates session options with credentials from a pre-generated token if one
/// is given, and from a development token server otherwise.
fn create_options(args: Args) -> Result<SessionOptions, Box<dyn Error>> {
    if let (Some(url), Some(token)) = (args.url, args.token) {
        return Ok(SessionOptions::new_fixed_source(literal(url, token)));
    }
    let token_server_id = args
        .token_server_id
        .ok_or("pass --token-server-id, or --url and --token (or set the LIVEKIT_* variables)")?;
    let mut fetch_options = TokenSourceFetchOptions::new();
    if let Some(agent_name) = args.agent_name {
        fetch_options = fetch_options.with_agent_name(agent_name);
    }
    Ok(SessionOptions::new_configurable_source(
        development_token_server(token_server_id),
        fetch_options,
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
    let args = Args::parse();

    // By default the session captures the microphone and plays the agent's
    // audio, with echo cancellation between the two. `--text` keeps the
    // conversation to typed lines, e.g. on a machine without audio devices.
    let text = args.text;
    let session_options = create_options(args)?
        .with_agent_connect_timeout(Duration::from_secs(15))
        .with_platform_audio(!text);
    let session = Session::new(session_options);
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
                let current = state.borrow_and_update().clone();
                println!("{:?}", session);
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
