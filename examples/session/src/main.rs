use std::error::Error;
use std::{env, time::Duration};

use livekit::options::TrackPublishOptions;
use livekit::prelude::{LocalAudioTrack, LocalTrack, PlatformAudio, TrackSource};
use livekit_session::{
    Message, MessageOrigin, Session, SessionEvent, SessionOptions, SessionState,
};

use livekit_token_source::{development_token_server, literal, TokenSourceFetchOptions};
use tokio::io::{AsyncBufReadExt, BufReader};

/// Creates a session reading credentials from a pre-generated token if one is
/// given, and from a development token server otherwise.
fn create_session(options: SessionOptions) -> Result<Session, Box<dyn Error>> {
    if let (Ok(url), Ok(token)) = (env::var("LIVEKIT_URL"), env::var("LIVEKIT_TOKEN")) {
        return Ok(Session::new(literal(url, token), options));
    }
    let token_server_id = env::var("LIVEKIT_TOKEN_SERVER_ID")
        .map_err(|_| "set LIVEKIT_TOKEN_SERVER_ID, or LIVEKIT_URL and LIVEKIT_TOKEN")?;
    let source = development_token_server(token_server_id);
    let session = match env::var("LIVEKIT_AGENT_NAME") {
        Ok(agent_name) => Session::with_agent(source, &agent_name, options),
        Err(_) => Session::with_fetch_options(source, TokenSourceFetchOptions::new(), options),
    };
    Ok(session)
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

/// Publishes the platform microphone so the agent can hear the user.
async fn publish_microphone(
    session: &Session,
    audio: &PlatformAudio,
) -> Result<(), Box<dyn Error>> {
    let room = session.room().ok_or("session has no room")?;
    let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
    let options = TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() };
    room.local_participant().publish_track(LocalTrack::Audio(track), options).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // Platform audio captures the microphone and plays the agent's audio, with
    // echo cancellation between the two. It has to exist before the room
    // connects, and stay alive for as long as the session runs.
    let voice = env::args().any(|arg| arg == "--voice");
    let audio = voice.then(PlatformAudio::new).transpose()?;

    let mut options = SessionOptions::default();
    options.agent_connect_timeout = Duration::from_secs(15);
    let session = create_session(options)?;
    let mut events = session.subscribe();
    session.start().await?;
    if let Some(audio) = &audio {
        publish_microphone(&session, audio).await?;
    }

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
