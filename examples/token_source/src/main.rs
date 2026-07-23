use livekit_token_source::{TokenSourceEndpoint, TokenSourceLiteral, TokenSourceResponse, TokenSourceSandbox, TokenSourceFetchOptions};

#[tokio::main]
async fn main() {
    // =======================================================
    let literal = TokenSourceLiteral::new(TokenSourceResponse{
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

    let options = TokenSourceFetchOptions { 
        agent_name: Some("Church".to_string()),
        ..Default::default()
    };

    // =======================================================
    let sandbox = TokenSourceSandbox::new("test1-xqsb8v".to_string());
    match sandbox.fetch(options.clone()).await {
        Ok(response) => {
            let url = response.server_url;
            let token = response.participant_token;
            println!("From Sandbox: {url} and token: {token}\n");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }

    // =======================================================
    let endpoint = TokenSourceEndpoint::new(
        "https://cloud-api.livekit.io/api/v2/sandbox/connection-details".to_string(),
        "X-Sandbox-ID".to_string(), "test1-xqsb8v".to_string()
    );
    match endpoint.fetch(options).await {
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