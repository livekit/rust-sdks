pub mod google {
    pub mod protobuf {
        include!(concat!(env!("OUT_DIR"), "/google.protobuf.rs"));
    }
}

pub mod livekit {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

pub use livekit::*;
