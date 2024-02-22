#[cfg(all(feature = "tokio", feature = "async"))]
compile_error!("Cannot compile livekit with both tokio and async_std support");

#[cfg(feature = "tokio")]
mod tokio;

#[cfg(feature = "tokio")]
pub use tokio::*;

#[cfg(feature = "async")]
mod async_std;

#[cfg(feature = "async")]
pub use async_std::*;