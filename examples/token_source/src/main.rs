use std::future;

use livekit::prelude::*;
use livekit_api::access_token;
use livekit::token_source::{
    MinterCredentials,
    MinterCredentialsEnvironment,
    TokenSourceCustom,
    TokenSourceCustomMinter,
    TokenSourceEndpoint,
    TokenSourceFetchOptions,
    TokenSourceLiteral,
    TokenSourceMinter,
    TokenSourceSandboxTokenServer,
    TokenSourceResponse,
};

#[tokio::main]
async fn main() {
    env_logger::init();

    let fetch_options = TokenSourceFetchOptions::default().with_agent_name("voice ai quickstart");

    let literal = TokenSourceLiteral::new(TokenSourceResponse {
        server_url: "...".into(),
        participant_token: "...".into(),
    });
    let literal_response = literal.fetch().await;
    log::info!("Example TokenSourceLiteral response: {:?}", literal_response);

    let minter = TokenSourceMinter::default();
    let minter_response = minter.fetch(&fetch_options).await;
    log::info!("Example TokenSourceMinter::default() result: {:?}", minter_response);

    let minter_literal_credentials = TokenSourceMinter::new(MinterCredentials::new("server url", "api key", "api secret"));
    let minter_literal_credentials_response = minter_literal_credentials.fetch(&fetch_options).await;
    log::info!("Example TokenSourceMinter / literal MinterCredentials result: {:?}", minter_literal_credentials_response);

    let minter_env = TokenSourceMinter::new(MinterCredentialsEnvironment::new("SERVER_URL", "API_KEY", "API_SECRET"));
    let minter_env_response = minter_env.fetch(&fetch_options).await;
    log::info!("Example TokenSourceMinter / MinterCredentialsEnvironment result: {:?}", minter_env_response);

    let minter_literal_custom = TokenSourceMinter::new(|| MinterCredentials::new("server url", "api key", "api secret"));
    let minter_literal_custom_response = minter_literal_custom.fetch(&fetch_options).await;
    log::info!("Example TokenSourceMinter / custom credentials result: {:?}", minter_literal_custom_response);

    let custom_minter = TokenSourceCustomMinter::<_, MinterCredentialsEnvironment>::new(|access_token| {
        access_token
            .with_identity("rust-bot")
            .with_name("Rust Bot")
            .with_grants(access_token::VideoGrants {
                room_join: true,
                room: "my-room".to_string(),
                ..Default::default()
            })
            .to_jwt()
    });
    let custom_minter_response = custom_minter.fetch().await;
    log::info!("Example TokenSourceCustomMinter response: {:?}", custom_minter_response);

    let endpoint = TokenSourceEndpoint::new("https://example.com/my/example/auth/endpoint");
    let endpoint_response = endpoint.fetch(&fetch_options).await;
    log::info!("Example TokenSourceMinter response: {:?}", endpoint_response);

    let sandbox_token_server = TokenSourceSandboxTokenServer::new("SANDBOX ID HERE");
    let sandbox_token_server_response = sandbox_token_server.fetch(&fetch_options).await;
    log::info!("Example TokenSourceSandboxTokenServer: {:?}", sandbox_token_server_response);

    // let foo = Box::pin(async |options: &TokenSourceFetchOptions| {
    //     Ok(TokenSourceResponse::new("...", "... _options should be encoded in here ..."))
    // });

    // // TODO: custom
    // let custom = TokenSourceCustom::new(foo);
    // let _ = custom.fetch(&fetch_options).await;

    let custom = TokenSourceCustom::new(|_options| {
        Box::pin(future::ready(
            Ok(TokenSourceResponse {
                server_url: "...".into(),
                participant_token: "... _options should be encoded in here ...".into(),
            })
        ))
    });
    let custom_response = custom.fetch(&fetch_options).await;
    log::info!("Example TokenSourceCustomResponse: {:?}", endpoint_response);
}
