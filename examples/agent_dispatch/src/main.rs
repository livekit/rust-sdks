use clap::Parser;
use livekit_api::services::agent_dispatch::AgentDispatchClient;
use livekit_protocol as proto;
use std::env;

#[derive(Parser, Debug)]
#[command(author, version, about = "Create an Agent Dispatch in a LiveKit room", long_about = None)]
struct Args {
    /// LiveKit server URL (can also be set via LIVEKIT_URL)
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (can also be set via LIVEKIT_API_KEY)
    #[arg(long, alias = "key")]
    api_key: Option<String>,

    /// LiveKit API secret (can also be set via LIVEKIT_API_SECRET)
    #[arg(long, alias = "secret")]
    api_secret: Option<String>,

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

    // Resolve connection + credentials from CLI or env vars (matching other examples)
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LiveKit URL must be provided via --url or LIVEKIT_URL env var");
    let host = normalize_host(&url);

    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("API key must be provided via --api-key or LIVEKIT_API_KEY env var");
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("API secret must be provided via --api-secret or LIVEKIT_API_SECRET env var");

    let room = args.room_name;
    let agent_name = args.agent_name;

    // Instantiate the AgentDispatch service client
    let client = AgentDispatchClient::with_api_key(&host, &api_key, &api_secret);

    // Create a dispatch for the given agent into the room
    let req = proto::CreateAgentDispatchRequest { agent_name, room, ..Default::default() };

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
