extern crate core;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

mod rtc_engine;
mod signal_client;
mod room;

pub mod webrtc {
    pub use livekit_webrtc::*;
}

pub use room::*;

/// `use livekit::prelude::*;` to import livekit types
pub mod prelude;
