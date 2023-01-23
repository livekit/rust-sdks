pub type FFIHandle = u32;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

mod conversion;
mod server;
