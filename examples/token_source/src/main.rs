use async_trait::async_trait;
use livekit_token_source::{
    TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed, TokenSourceResponse,
    TokenSourceResult,
};

/// An example for a custom token source that reads credentials from a JSON file, e.g.
/// `{"server_url": "wss://...", "participant_token": "..."}`.
struct FileTokenSource {
    path: std::path::PathBuf,
}

#[async_trait]
impl TokenSourceFixed for FileTokenSource {
    async fn fetch(&self) -> TokenSourceResult<TokenSourceResponse> {
        let contents = std::fs::read_to_string(&self.path).map_err(serde_json::Error::io)?;
        let response = serde_json::from_str(&contents)?;
        Ok(response)
    }
}

#[tokio::main]
async fn main() {
    // A literal token source returns a fixed set of pre-provisioned credentials.
    let literal =
        livekit_token_source::literal("wss://example.livekit.cloud", "<a pre-generated token>");
    match literal.fetch().await {
        Ok(response) => println!(
            "literal: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("literal fetch failed: {error}"),
    }

    // A custom token source can procure credentials from anywhere; this one
    // reads them from a JSON file next to this example's Cargo.toml.
    let file_source =
        FileTokenSource { path: concat!(env!("CARGO_MANIFEST_DIR"), "/token.json").into() };
    match file_source.fetch().await {
        Ok(response) => println!(
            "file: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("file fetch failed: {error}"),
    }

    // The remaining sources query LiveKit's development token server, which
    // requires the ID of a sandbox created in your LiveKit Cloud project.
    let sandbox_id = "your sandbox id".to_string();

    let options = TokenSourceFetchOptions::new()
        .with_room_name("example-room")
        .with_participant_identity("example-user");

    // Development token server: for prototyping only, NOT for production use.
    let development_token_server =
        livekit_token_source::development_token_server(sandbox_id.clone());
    match development_token_server.fetch(&options).await {
        Ok(response) => println!(
            "development token server: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("development token server fetch failed: {error}"),
    }

    // Caching: wrap any configurable source with `.cached()` to reuse
    // credentials until the token expires; the second fetch below is served
    // from the cache without hitting the server.
    let cached = livekit_token_source::development_token_server(sandbox_id.clone()).cached();
    for attempt in 1..=2 {
        match cached.fetch(&options).await {
            Ok(response) => println!(
                "cached fetch #{attempt}: server_url={} participant_token={}",
                response.server_url, response.participant_token
            ),
            Err(error) => eprintln!("cached fetch #{attempt} failed: {error}"),
        }
    }

    // Endpoint: POSTs the fetch options to a token endpoint using the standard
    // format; here pointed at the same development token server.
    let endpoint = livekit_token_source::endpoint(
        "https://cloud-api.livekit.io/api/v2/sandbox/connection-details",
    )
    .with_header("X-Sandbox-ID", sandbox_id);
    match endpoint.fetch(&options).await {
        Ok(response) => println!(
            "endpoint: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("endpoint fetch failed: {error}"),
    }
}
