#[cfg(not(target_arch = "wasm32"))]
#[path = ""]
mod platform {
    mod native;
    pub use native::*;
}

#[cfg(target_arch = "wasm32")]
#[path = ""]
mod platform {
    mod web;
    pub use web::*;
}

pub mod data_channel;

// pub mod data_channel;
// pub mod jsep;
// pub mod media_stream;
// pub mod peer_connection;
// pub mod peer_connection_factory;
// pub mod prelude;
// pub mod rtc_error;
// pub mod rtp_parameters;
// pub mod rtp_receiver;
// pub mod rtp_sender;
// pub mod rtp_transceiver;
// pub mod video_frame;
// pub mod video_frame_buffer;
// pub mod webrtc;
// pub mod yuv_helper;

macro_rules! impl_sys_conversion {
    ($sys:ty, $safe:ty, [$($variant:ident),+]) => {
        impl From<$sys> for $safe {
            fn from(value: $sys) -> Self {
                match value {
                    $(<$sys>::$variant => Self::$variant,)+
                    _ => panic!("invalid value from sys"),
                }
            }
        }

        impl From<$safe> for $sys {
            fn from(value: $safe) -> Self {
                match value {
                    $(<$safe>::$variant => Self::$variant,)+
                }
            }
        }
    };
}

pub(crate) use impl_sys_conversion;
