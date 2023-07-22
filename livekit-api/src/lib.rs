#[cfg(feature = "access-token")]
pub mod access_token;

#[cfg(feature = "services")]
pub mod services;

#[cfg(feature = "signal-client")]
pub mod signal_client;

#[cfg(feature = "webhooks")]
pub mod webhooks;

#[allow(dead_code)]
pub(crate) fn get_env_keys() -> Result<(String, String), std::env::VarError> {
    let api_key = std::env::var("LIVEKIT_API_KEY")?;
    let api_secret = std::env::var("LIVEKIT_API_SECRET")?;
    Ok((api_key, api_secret))
}
