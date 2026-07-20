use livekit::token_source::{TokenSourceLiteral, TokenSourceResponse};

fn main() {
    println!("Hello, world!");
    let test = TokenSourceLiteral::new(TokenSourceResponse{
        server_url: "Hello Max".to_string(),
        participant_token: "Hello Max".to_string()
    });
    match test.result {
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