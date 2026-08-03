#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("no HTTP client available; enable a livekit-net backend feature or call livekit_net::set_http_client")]
    TransportNotConfigured,

    #[error("failed to fetch token: {0}")]
    Transport(#[from] livekit_net::TransportError),

    #[error("failed to serialize request / parse response: {0}")]
    Json(#[from] serde_json::Error),

    #[error("token server returned {status}: {body}")]
    Server{ status: u16, body: String },
}
