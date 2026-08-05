use livekit_token_source::{
    TokenSource, TokenSourceConfigurable, TokenSourceFetchOptions, TokenSourceFixed,
    TokenSourceResponse,
};

#[tokio::main]
async fn main() {
    // A literal token source returns a fixed set of pre-provisioned credentials.
    let literal = TokenSource::literal(TokenSourceResponse {
        server_url: "wss://example.livekit.cloud".to_string(),
        participant_token: "<a pre-generated token>".to_string(),
    });
    match literal.fetch().await {
        Ok(response) => println!(
            "literal: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("literal fetch failed: {error}"),
    }

    // The remaining sources query LiveKit's development token server, which
    // requires the ID of a sandbox created in your LiveKit Cloud project.
    let Ok(sandbox_id) = std::env::var("LIVEKIT_SANDBOX_ID") else {
        eprintln!("set LIVEKIT_SANDBOX_ID to run the remaining examples");
        return;
    };

    let options = TokenSourceFetchOptions::new()
        .with_room_name("example-room")
        .with_participant_identity("example-user");

    // Development token server: for prototyping only, NOT for production use.
    let development_token_server = TokenSource::development_token_server(sandbox_id.clone());
    match development_token_server.fetch(&options).await {
        Ok(response) => println!(
            "development token server: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("development token server fetch failed: {error}"),
    }

    // Endpoint: POSTs the fetch options to a token endpoint using the standard
    // format; here pointed at the same development token server.
    let endpoint = TokenSource::endpoint(
        "https://cloud-api.livekit.io/api/v2/sandbox/connection-details",
        vec![("X-Sandbox-ID".to_string(), sandbox_id)],
    );
    match endpoint.fetch(&options).await {
        Ok(response) => println!(
            "endpoint: server_url={} participant_token={}",
            response.server_url, response.participant_token
        ),
        Err(error) => eprintln!("endpoint fetch failed: {error}"),
    }
}
