use cxx::UniquePtr;
use libwebrtc_sys::data_channel as sys_dc;

pub struct DataChannel {
    cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>,
}

impl DataChannel {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>) -> Self {
        Self {
            cxx_handle,
        }
    }
}