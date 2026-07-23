#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TokenSourceError {
    #[error("failed to fetch token: {message}")]
    Request { message: String },

    #[error("token server returned {status}: {body}")]
    Server { status: u16, body: String },

    #[error("error A occurred")]
    ErrorA,
    #[error("error B occurred")]
    ErrorB,
}

impl From<reqwest::Error> for TokenSourceError {
    fn from(error: reqwest::Error) -> Self {
        TokenSourceError::Request { message: error.to_string() }
    }
}
