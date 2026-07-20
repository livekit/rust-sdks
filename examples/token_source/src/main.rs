use livekit_token_source::{TokenSourceLiteral, TokenSourceSandbox, TokenSourceResponse};

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    // =======================================================
    let test = TokenSourceLiteral::new(TokenSourceResponse{
        server_url: "Hello Max".to_string(),
        participant_token: "Hello Max".to_string()
    });
    match test.fetch() {
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
    let test2 = TokenSourceSandbox::new("test1-xqsb8v".to_string());
    match test2.fetch().await {
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