use std::fmt::Debug;
use thiserror::Error;

pub mod room_client;
mod twirp_client;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid environment: {0}")]
    Env(#[from] std::env::VarError),
}

struct ServiceBase {
    api_key: String,
    api_secret: String,
}

impl Debug for ServiceBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceBase")
            .field("api_key", &self.api_key)
            .finish()
    }
}

impl ServiceBase {
    pub fn with_api_key(api_key: &str, api_secret: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
        }
    }

    pub fn new() -> Result<Self, ServiceError> {
        let (api_key, api_secret) = super::get_env_keys()?;
        Ok(Self::with_api_key(&api_key, &api_secret))
    }
}
