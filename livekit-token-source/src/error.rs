#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("failed to fetch token: {0}")]
    Request(#[from] reqwest::Error),

    #[error("token server returned {status}: {body}")]
    Server{ status: u16, body: String },

    #[error("error A occurred")]
    ErrorA,
    #[error("error B occurred")]
    ErrorB,
}