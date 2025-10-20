#[cfg(any(
    all(feature = "tokio", feature = "async"),
    all(feature = "tokio", feature = "dispatcher"),
    all(feature = "dispatcher", feature = "async")
))]
compile_error!("Cannot compile livekit with multiple runtimes");

#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
mod tokio;
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
pub use tokio::*;

#[cfg(feature = "async")]
mod async_std;
#[cfg(feature = "async")]
pub use async_std::*;

#[cfg(feature = "dispatcher")]
mod dispatcher;
#[cfg(feature = "dispatcher")]
pub use dispatcher::*;
