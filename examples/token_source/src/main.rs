use livekit_token_source::{TokenSourceEndpoint, TokenSourceLiteral, TokenSourceResponse, TokenSourceSandbox};

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    // =======================================================
    let literal = TokenSourceLiteral::new(TokenSourceResponse{
        server_url: "Hello Max".to_string(),
        participant_token: "Hello Max".to_string()
    });
    match literal.fetch() {
        Ok(response) => {
            let url = &response.server_url;
            let token = &response.participant_token;
            println!("The response is server_url: {url} and token: {token}");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }
    match literal.fetch() {
        Ok(response) => {
            let url = &response.server_url;
            let token = &response.participant_token;
            println!("The response is server_url: {url} and token: {token}");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }

    // =======================================================
    let sandbox = TokenSourceSandbox::new("test1-xqsb8v".to_string());
    match sandbox.fetch().await {
        Ok(response) => {
            let url = response.server_url;
            let token = response.participant_token;
            println!("The response is server_url: {url} and token: {token}");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }

    // =======================================================
    let endpoint = TokenSourceEndpoint::new(
        "https://cloud-api.livekit.io/api/v2/sandbox/connection-details".to_string(), 
        ("X-Sandbox-ID".to_string(), "test1-xqsb8v".to_string())
    );
    match endpoint.fetch().await {
        Ok(response) => {
            let url = response.server_url;
            let token = response.participant_token;
            println!("The response is server_url: {url} and token: {token}");
        },
        Err(error) => {
            println!("I got error {error}")
        },
    }
}