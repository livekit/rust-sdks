#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("failed to fetch token: {0}")]
    Request(#[from] reqwest::Error),
    #[error("error A occurred")]
    ErrorA,
    #[error("error B occurred")]
    ErrorB,
}