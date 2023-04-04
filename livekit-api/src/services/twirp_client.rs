use std::fmt::Display;

use reqwest::{header::HeaderMap, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TwirpError {
    #[error("failed to execute the request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("twirp error: {0}")]
    Twirp(TwirpErrorCode),
}

#[derive(Debug, Deserialize)]
pub struct TwirpErrorCode {
    pub code: String,
    pub msg: String,
}

impl TwirpErrorCode {
    pub const CANCELED: &'static str = "canceled";
    pub const UNKNOWN: &'static str = "unknown";
    pub const INVALID_ARGUMENT: &'static str = "invalid_argument";
    pub const MALFORMED: &'static str = "malformed";
    pub const DEADLINE_EXCEEDED: &'static str = "deadline_exceeded";
    pub const NOT_FOUND: &'static str = "not_found";
    pub const BAD_ROUTE: &'static str = "bad_route";
    pub const ALREADY_EXISTS: &'static str = "already_exists";
    pub const PERMISSION_DENIED: &'static str = "permission_denied";
    pub const UNAUTHENTICATED: &'static str = "unauthenticated";
    pub const RESOURCE_EXHAUSTED: &'static str = "resource_exhausted";
    pub const FAILED_PRECONDITION: &'static str = "failed_precondition";
    pub const ABORTED: &'static str = "aborted";
    pub const OUT_OF_RANGE: &'static str = "out_of_range";
    pub const UNIMPLEMENTED: &'static str = "unimplemented";
    pub const INTERNAL: &'static str = "internal";
    pub const UNAVAILABLE: &'static str = "unavailable";
    pub const DATA_LOSS: &'static str = "dataloss";
}

impl Display for TwirpErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.msg)
    }
}

pub type TwirpResult<T> = Result<T, TwirpError>;

#[derive(Debug)]
pub struct TwirpClient {
    host: String,
    pkg: String,
    prefix: String,
    client: reqwest::Client,
}

impl TwirpClient {
    pub fn new(host: &str, pkg: &str, prefix: &str) -> Self {
        Self {
            host: host.to_owned(),
            pkg: pkg.to_owned(),
            prefix: prefix.to_owned(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn request<D: Serialize, R: DeserializeOwned>(
        self,
        service: &str,
        method: &str,
        data: D,
        headers: HeaderMap,
    ) -> TwirpResult<R> {
        let resp = self
            .client
            .get(format!(
                "{}/{}.{}/{}",
                self.prefix, self.pkg, service, method
            ))
            .headers(headers)
            .json(&data)
            .send()
            .await?;

        if resp.status() == StatusCode::OK {
            Ok(resp.json().await?)
        } else {
            let err: TwirpErrorCode = resp.json().await?;
            Err(TwirpError::Twirp(err))
        }
    }
}
