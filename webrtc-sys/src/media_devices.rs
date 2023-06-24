use crate::impl_thread_safety;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[repr(i32)]
    pub enum DeviceFacing {
        Unknown,
        User,
        Environment,
    }

    #[repr(i32)]
    pub enum DeviceKind {
        AudioInput,
        AudioOutput,
        VideoInput,
    }

    pub struct DeviceInfo {
        pub id: String,
        pub name: String,
        pub kind: DeviceKind,
        pub facing: DeviceFacing, // If video input
    }

    unsafe extern "C++" {
        include!("livekit/media_devices.h");

    }
}
