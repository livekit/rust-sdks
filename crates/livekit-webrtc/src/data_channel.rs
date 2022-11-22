use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::{Arc, Mutex};

use cxx::UniquePtr;
use log::trace;

use libwebrtc_sys::data_channel as sys_dc;
pub use sys_dc::ffi::{DataState, Priority};

pub struct DataChannel {
    cxx_handle: UniquePtr<sys_dc::ffi::DataChannel>,
    observer: Box<InternalDataChannelObserver>,

    // Keep alive for C++
    native_observer: UniquePtr<sys_dc::ffi::NativeDataChannelObserver>,
}

impl Debug for DataChannel {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("DataChannel")
            .field("label", &self.label())
            .finish()
    }
}

#[derive(Debug)]
pub struct DataSendError;

impl Display for DataSendError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "failed to send data to the DataChannel")
    }
}

impl Error for DataSendError {}

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

    pub fn send(&mut self, data: &[u8], binary: bool) -> Result<(), DataSendError> {
        let buffer = sys_dc::ffi::DataBuffer {
            ptr: data.as_ptr(),
            len: data.len(),
            binary,
        };

        self.cxx_handle
            .pin_mut()
            .send(&buffer)
            .then_some(())
            .ok_or(DataSendError {})
    }

    pub fn label(&self) -> String {
        self.cxx_handle.label()
    }

    pub fn state(&self) -> DataState {
        self.cxx_handle.state()
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

    pub fn on_buffered_amount_change(&mut self, handler: OnBufferedAmountChangeHandler) {
        *self
            .observer
            .on_buffered_amount_change_handler
            .lock()
            .unwrap() = Some(handler);
    }
}

impl Drop for DataChannel {
    fn drop(&mut self) {
        self.cxx_handle.pin_mut().unregister_observer();
    }
}

pub type OnStateChangeHandler = Box<dyn FnMut() + Send + Sync>;
pub type OnMessageHandler = Box<dyn FnMut(&[u8], bool) + Send + Sync>;
pub type OnBufferedAmountChangeHandler = Box<dyn FnMut(u64) + Send + Sync>;

#[derive(Default)]
struct InternalDataChannelObserver {
    on_state_change_handler: Mutex<Option<OnStateChangeHandler>>,
    on_message_handler: Mutex<Option<OnMessageHandler>>,
    on_buffered_amount_change_handler: Mutex<Option<OnBufferedAmountChangeHandler>>,
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

#[derive(Debug)]
pub struct DataChannelInit {
    #[deprecated]
    pub reliable: bool,
    pub ordered: bool,
    pub max_retransmit_time: Option<i32>,
    pub max_retransmits: Option<i32>,
    pub protocol: String,
    pub negotiated: bool,
    pub id: i32,
    pub priority: Option<Priority>,
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
