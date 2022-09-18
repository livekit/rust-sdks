use cxx::UniquePtr;
use libwebrtc_sys::data_channel as sys_dc;

pub use sys_dc::ffi::Priority;

pub struct DataChannel {
    cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>,
}

impl DataChannel {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>) -> Self {
        Self { cxx_handle }
    }
}

#[derive(Debug)]
pub struct DataChannelInit {
    #[deprecated]
    reliable: bool,
    ordered: bool,
    max_retransmit_time: Option<i32>,
    max_retransmits: Option<i32>,
    protocol: String,
    negotiated: bool,
    id: i32,
    priority: Option<Priority>,
}

impl Default for DataChannelInit {
    fn default() -> Self {
        Self {
            reliable: false,
            ordered: true,
            max_retransmit_time: None,
            max_retransmits: None,
            protocol: "".to_string(),
            negotiated: false,
            id: -1,
            priority: None,
        }
    }
}

impl From<DataChannelInit> for sys_dc::ffi::DataChannelInit {
    fn from(init: DataChannelInit) -> Self {
        Self {
            reliable: init.reliable,
            ordered: init.ordered,
            has_max_retransmit_time: init.max_retransmit_time.is_some(),
            max_retransmit_time: init.max_retransmit_time.unwrap_or_default(),
            has_max_retransmits: init.max_retransmits.is_some(),
            max_retransmits: init.max_retransmits.unwrap_or_default(),
            protocol: init.protocol,
            negotiated: init.negotiated,
            id: init.id,
            has_priority: init.priority.is_some(),
            priority: init.priority.unwrap_or(Priority::Low),
        }
    }
}
