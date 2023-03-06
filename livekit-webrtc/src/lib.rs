use thiserror::Error;

#[cfg_attr(target_arch = "wasm32", path = "web/mod.rs")]
#[cfg_attr(not(target_arch = "wasm32"), path = "native/mod.rs")]
mod imp;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MediaType {
    Audio,
    Video,
    Data,
    Unsupported,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtcErrorType {
    Internal,
    InvalidSdp,
    InvalidState,
}

#[derive(Error, Debug)]
#[error("an RtcError occured: {error_type:?} - {message}")]
pub struct RtcError {
    pub error_type: RtcErrorType,
    pub message: String,
}

pub mod data_channel;
pub mod ice_candidate;
pub mod media_stream;
pub mod peer_connection;
pub mod peer_connection_factory;
pub mod prelude;
pub mod rtp_parameters;
pub mod rtp_receiver;
pub mod rtp_sender;
pub mod rtp_transceiver;
pub mod session_description;
pub mod video_frame;
pub mod video_sink;

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    pub use crate::imp::yuv_helper;
}
