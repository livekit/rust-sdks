use clap::Parser;
use livekit_api::services::agent_dispatch::AgentDispatchClient;
use livekit_protocol as proto;
use std::env;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create an Agent Dispatch in a LiveKit room", long_about = None)]
struct Args {
    /// LiveKit server URL
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key
    #[arg(long, alias = "key", env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret
    #[arg(long, alias = "secret", env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// LiveKit room name to dispatch the agent to
    #[arg(long, default_value = "my-room")]
    room_name: String,

    /// Registered agent name to dispatch
    #[arg(long, default_value = "my-agent")]
    agent_name: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    let host = normalize_host(&args.url);

    // Instantiate the AgentDispatch service client
    let client = AgentDispatchClient::with_api_key(&host, &args.api_key, &args.api_secret);

    // Create a dispatch for the given agent into the room
    let req = proto::CreateAgentDispatchRequest {
        agent_name: args.agent_name,
        room: args.room_name,
        ..Default::default()
    };

    match client.create_dispatch(req).await {
        Ok(dispatch) => {
            println!("Created dispatch: {:?}", dispatch);
        }
        Err(e) => {
            eprintln!("Failed to create dispatch: {}", e);
        }
    }
}

fn normalize_host(url: &str) -> String {
    // Convert websocket scheme to HTTP for Twirp services
    if let Some(rest) = url.strip_prefix("wss://") {
        return format!("https://{}", rest.trim_end_matches("/rtc"));
    }
    if let Some(rest) = url.strip_prefix("ws://") {
        return format!("http://{}", rest.trim_end_matches("/rtc"));
    }
    url.trim_end_matches("/rtc").to_string()
}
