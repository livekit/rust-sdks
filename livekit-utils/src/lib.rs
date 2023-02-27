pub mod enum_dispatch;

#[cfg(not(target_arch = "wasm32"))]
pub mod observer;
