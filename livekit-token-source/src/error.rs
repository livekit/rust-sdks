use thiserror::Error;

#[derive(Debug, thiserror::Error)]
pub enum TokenSourceError {
    #[error("error A occurred")]
    ErrorA,
    #[error("error B occurred")]
    ErrorB,
}