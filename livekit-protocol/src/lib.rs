pub mod enum_dispatch;
pub mod observer;

#[cfg(not(feature = "json"))]
mod livekit {
    pub mod google {
        pub mod protobuf {
            include!(concat!(env!("OUT_DIR"), "/google.protobuf.rs"));
        }
    }

    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

#[cfg(feature = "json")]
mod livekit {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
    include!(concat!(env!("OUT_DIR"), "/livekit.serde.rs"));
}

pub use livekit::*;
