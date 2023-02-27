use crate::data_channel::{
    DataBuffer, DataChannelError, DataChannelInit, DataState, OnBufferedAmountChange, OnMessage,
    OnStateChange,
};
use cxx::SharedPtr;
use std::str;
use std::sync::{Arc, Mutex};
use webrtc_sys::data_channel as sys_dc;

impl From<sys_dc::ffi::DataState> for DataState {
    fn from(value: sys_dc::ffi::DataState) -> Self {
        match value {
            sys_dc::ffi::DataState::Connecting => Self::Connecting,
            sys_dc::ffi::DataState::Open => Self::Open,
            sys_dc::ffi::DataState::Closing => Self::Closing,
            sys_dc::ffi::DataState::Closed => Self::Closed,
            _ => panic!("unknown data channel state"),
        }
    }
}

impl From<DataChannelInit> for sys_dc::ffi::DataChannelInit {
    fn from(value: DataChannelInit) -> Self {
        Self {
            ordered: value.ordered,
            has_max_retransmit_time: value.max_retransmit_time.is_some(),
            max_retransmit_time: value.max_retransmit_time.unwrap_or_default(),
            has_max_retransmits: value.max_retransmits.is_some(),
            max_retransmits: value.max_retransmits.unwrap_or_default(),
            protocol: value.protocol,
            id: value.id,
            has_priority: false,
            priority: sys_dc::ffi::Priority::Medium,
            negotiated: value.negotiated,
        }
    }
}

#[derive(Clone)]
pub struct DataChannel {
    #[allow(dead_code)]
    native_observer: SharedPtr<sys_dc::ffi::NativeDataChannelObserver>,
    observer: Arc<DataChannelObserver>,

    pub(crate) sys_handle: SharedPtr<sys_dc::ffi::DataChannel>,
}

impl DataChannel {
    pub fn configure(sys_handle: SharedPtr<sys_dc::ffi::DataChannel>) -> Self {
        unsafe {
            let observer = Arc::new(DataChannelObserver::default());
            let dc = Self {
                sys_handle: sys_handle.clone(),
                native_observer: sys_dc::ffi::create_native_data_channel_observer(
                    Box::new(sys_dc::DataChannelObserverWrapper::new(observer.clone())),
                    &*sys_handle,
                ),
                observer,
            };

            dc.sys_handle
                .register_observer(&dc.native_observer as *const _ as *mut _);
            dc
        }
    }

    pub fn send(&self, data: &[u8], binary: bool) -> Result<(), DataChannelError> {
        if !binary {
            str::from_utf8(data)?;
        }

        let buffer = sys_dc::ffi::DataBuffer {
            ptr: data.as_ptr(),
            len: data.len(),
            binary,
        };

        self.sys_handle
            .send(&buffer)
            .then_some(())
            .ok_or(DataChannelError::Send)
    }

    pub fn label(&self) -> String {
        self.sys_handle.label()
    }

    pub fn state(&self) -> DataState {
        self.sys_handle.state().into()
    }

    pub fn close(&self) {
        self.sys_handle.close();
    }

    pub fn on_state_change(&self, handler: Option<OnStateChange>) {
        *self.observer.state_change_handler.lock().unwrap() = handler;
    }

    pub fn on_message(&self, handler: Option<OnMessage>) {
        *self.observer.message_handler.lock().unwrap() = handler;
    }

    pub fn on_buffered_amount_change(&self, handler: Option<OnBufferedAmountChange>) {
        *self.observer.buffered_amount_change_handler.lock().unwrap() = handler;
    }
}

#[derive(Default)]
struct DataChannelObserver {
    state_change_handler: Mutex<Option<OnStateChange>>,
    message_handler: Mutex<Option<OnMessage>>,
    buffered_amount_change_handler: Mutex<Option<OnBufferedAmountChange>>,
}

impl sys_dc::DataChannelObserver for DataChannelObserver {
    fn on_state_change(&self, state: sys_dc::ffi::DataState) {
        let mut handler = self.state_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    fn on_message(&self, data: &[u8], binary: bool) {
        let mut handler = self.message_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(DataBuffer { data, binary });
        }
    }

    fn on_buffered_amount_change(&self, sent_data_size: u64) {
        let mut handler = self.buffered_amount_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(sent_data_size);
        }
    }
}
