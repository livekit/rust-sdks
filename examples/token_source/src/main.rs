use livekit_token_source::{TokenSource, TokenSourceResponse, TokenSourceFetchOptions};

#[tokio::main]
async fn main() {
    // =======================================================
    let literal = TokenSource::literal(TokenSourceResponse{
        server_url: "< some server url >".to_string(),
        participant_token: "< some token >\n".to_string()
    });
    match literal.fetch() {
        Ok(response) => {
            let url = &response.server_url;
            let token = &response.participant_token;
            println!("From Literal: {url} and token: {token}");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }

    let options = TokenSourceFetchOptions::new()
        .with_agent_name("Church");

    // =======================================================
    let development_token_server = TokenSource::development_token_server("test1-xqsb8v".to_string());
    match development_token_server.fetch(&options).await {
        Ok(response) => {
            let url = response.server_url;
            let token = response.participant_token;
            println!("From Development Token Server: {url} and token: {token}\n");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }

    // =======================================================
    let endpoint = TokenSource::endpoint(
        "https://cloud-api.livekit.io/api/v2/sandbox/connection-details", 
        vec![("X-Sandbox-ID".to_string(), "test1-xqsb8v".to_string())]
    );
    match endpoint.fetch(&options).await {
        Ok(response) => {
            let url = response.server_url;
            let token = response.participant_token;
            println!("From Endpoint: {url} and token: {token}\n");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }
}