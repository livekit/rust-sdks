use std::fmt::{Debug, Formatter};
use cxx::UniquePtr;
use libwebrtc_sys::data_channel as sys_dc;
use log::trace;
use std::sync::{Arc, Mutex};

pub use sys_dc::ffi::Priority;

pub struct DataChannel {
    cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>,
    observer: Box<InternalDataChannelObserver>,

    // Keep alive for C++
    native_observer: UniquePtr<sys_dc::ffi::NativeDataChannelObserver>,
}

impl Debug for DataChannel {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "DataChannel [{:?}]", self.label())
    }
}

impl DataChannel {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>) -> Self {
        let mut observer = Box::new(InternalDataChannelObserver::default());

        let mut dc = unsafe {
            Self {
                cxx_handle,
                native_observer: sys_dc::ffi::create_native_data_channel_observer(Box::new(
                    sys_dc::DataChannelObserverWrapper::new(&mut *observer),
                )),
                observer,
            }
        };

        unsafe {
            dc.cxx_handle
                .pin_mut()
                .register_observer(dc.native_observer.pin_mut());
        }

        dc
    }

    pub fn send(&mut self, data: &[u8], binary: bool) -> bool {
        let buffer = sys_dc::ffi::DataBuffer {
            ptr: data.as_ptr(),
            len: data.len(),
            binary,
        };
        self.cxx_handle.pin_mut().send(&buffer)
    }

    pub fn label(&self) -> String {
        self.cxx_handle.label()
    }

    pub fn close(&mut self) {
        self.cxx_handle.pin_mut().close();
    }

    pub fn on_state_change(&mut self, handler: OnStateChangeHandler) {
        *self.observer.on_state_change_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_message(&mut self, handler: OnMessageHandler) {
        *self.observer.on_message_handler.lock().unwrap() = Some(handler);
    }

    pub fn on_buffer(&mut self, handler: OnBufferedAmountChangeHandler) {
        *self
            .observer
            .on_buffered_amount_change_handler
            .lock()
            .unwrap() = Some(handler);
    }
}

pub type OnStateChangeHandler = Box<dyn FnMut() + Send + Sync>;
pub type OnMessageHandler = Box<dyn FnMut(&[u8], bool) + Send + Sync>; // data, is_binary
pub type OnBufferedAmountChangeHandler = Box<dyn FnMut(u64) + Send + Sync>;

struct InternalDataChannelObserver {
    on_state_change_handler: Arc<Mutex<Option<OnStateChangeHandler>>>,
    on_message_handler: Arc<Mutex<Option<OnMessageHandler>>>,
    on_buffered_amount_change_handler: Arc<Mutex<Option<OnBufferedAmountChangeHandler>>>,
}

impl sys_dc::DataChannelObserver for InternalDataChannelObserver {
    fn on_state_change(&self) {
        trace!("DataChannel: on_state_change");
        let mut handler = self.on_state_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f();
        }
    }

    fn on_message(&self, data: &[u8], is_binary: bool) {
        trace!("DataChannel: on_message");
        let mut handler = self.on_message_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(data, is_binary);
        }
    }

    fn on_buffered_amount_change(&self, sent_data_size: u64) {
        trace!("DataChannel: on_buffered_amount_change");
        let mut handler = self.on_buffered_amount_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(sent_data_size);
        }
    }
}

impl Default for InternalDataChannelObserver {
    fn default() -> Self {
        Self {
            on_state_change_handler: Arc::new(Default::default()),
            on_message_handler: Arc::new(Default::default()),
            on_buffered_amount_change_handler: Arc::new(Default::default()),
        }
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
