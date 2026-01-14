// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::impl_thread_safety;
use crate::sys::{self, lkDataChannelObserver, lkDcState};
use serde_derive::Deserialize;
use std::{
    fmt::Debug,
    str::Utf8Error,
    sync::{Arc, Mutex},
};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataChannelState {
    Connecting,
    Open,
    Closing,
    Closed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
}

#[derive(Debug)]
pub struct DataBuffer<'a> {
    pub data: &'a [u8],
    pub binary: bool,
}

pub type OnStateChange = Box<dyn FnMut(DataChannelState) + Send + Sync>;
pub type OnMessage = Box<dyn FnMut(DataBuffer) + Send + Sync>;
pub type OnBufferedAmountChange = Box<dyn FnMut(u64) + Send + Sync>;

#[derive(Debug, Error)]
pub enum DataChannelError {
    #[error("failed to send data, dc not open? send buffer is full ?")]
    Send,
    #[error("only utf8 strings can be sent")]
    Utf8(#[from] Utf8Error),
}

#[derive(Clone, Debug)]
pub struct DataChannelInit {
    pub ordered: bool,
    pub reliable: bool,
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
            ordered: true,
            reliable: true,
            max_retransmit_time: None,
            max_retransmits: None,
            protocol: String::new(),
            negotiated: false,
            id: -1,
            priority: None,
        }
    }
}

impl From<lkDcState> for DataChannelState {
    fn from(value: lkDcState) -> Self {
        match value {
            lkDcState::LK_DC_STATE_CONNECTING => Self::Connecting,
            lkDcState::LK_DC_STATE_OPEN => Self::Open,
            lkDcState::LK_DC_STATE_CLOSING => Self::Closing,
            lkDcState::LK_DC_STATE_CLOSED => Self::Closed,
        }
    }
}

impl From<DataChannelInit> for sys::lkDataChannelInit {
    fn from(value: DataChannelInit) -> Self {
        sys::lkDataChannelInit {
            ordered: value.ordered,
            maxRetransmits: value.max_retransmits.unwrap_or(-1),
            reliable: value.reliable,
        }
    }
}

#[derive(Clone)]
pub struct DataChannel {
    observer: Arc<DataChannelObserver>,
    ffi: sys::RefCounted<sys::lkDataChannel>,
}

impl_thread_safety!(DataChannel, Send + Sync);

static DC_OBSERVER: sys::lkDataChannelObserver = lkDataChannelObserver {
    onStateChange: Some(DataChannelObserver::on_state_change),
    onMessage: Some(DataChannelObserver::on_message),
    onBufferedAmountChange: Some(DataChannelObserver::on_buffered_amount_change),
};

impl DataChannel {
    pub fn set_observer(&mut self, observer: Arc<DataChannelObserver>) {
        self.observer = observer;
    }

    pub fn configure(sys_handle: sys::RefCounted<sys::lkDataChannel>) -> Self {
        let observer = Arc::new(DataChannelObserver::default());
        let observer_ptr = Arc::into_raw(observer.clone());
        unsafe {
            sys::lkDcRegisterObserver(
                sys_handle.as_ptr(),
                &DC_OBSERVER,
                observer_ptr as *mut ::std::os::raw::c_void,
            );
        }
        Self { ffi: sys_handle, observer: observer }
    }

    pub fn send(&self, data: &[u8], binary: bool) -> Result<(), DataChannelError> {
        if !binary {
            str::from_utf8(data)?;
        }
        unsafe {
            sys::lkDcSendAsync(
                self.ffi.as_ptr(),
                data.as_ptr() as *const u8,
                data.len() as u64,
                binary,
                None,
                std::ptr::null_mut(),
            );
        }
        Ok(())
    }

    pub async fn send_async(&self, data: &[u8], binary: bool) -> Result<(), DataChannelError> {
        let (tx, mut rx) = mpsc::channel::<Result<(), DataChannelError>>(1);
        let tx_box = Box::new(tx);
        let userdata = Box::into_raw(tx_box) as *mut std::ffi::c_void;

        unsafe extern "C" fn on_complete(
            error: *mut sys::lkRtcError,
            userdata: *mut ::std::os::raw::c_void,
        ) {
            println!("on_complete called with error: {:?}", error);
            let tx: Box<mpsc::Sender<Result<(), DataChannelError>>> =
                Box::from_raw(userdata as *mut _);
            if error.is_null() {
                let _ = tx.blocking_send(Ok(()));
                return;
            }
            let _ = tx.blocking_send(Err(DataChannelError::Send));
        }

        unsafe {
            sys::lkDcSendAsync(
                self.ffi.as_ptr(),
                data.as_ptr() as *const u8,
                data.len() as u64,
                binary,
                Some(on_complete),
                userdata,
            );
        }

        rx.recv().await.unwrap()
    }

    pub fn id(&self) -> i32 {
        unsafe { sys::lkDcGetId(self.ffi.as_ptr()) }
    }

    pub fn label(&self) -> String {
        unsafe {
            let str_ptr = sys::lkDcGetLabel(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }

    pub fn state(&self) -> DataChannelState {
        let state = unsafe { sys::lkDcGetState(self.ffi.as_ptr()) };
        state.into()
    }

    pub fn close(&self) {
        unsafe { sys::lkDcClose(self.ffi.as_ptr()) };
    }

    pub fn buffered_amount(&self) -> u64 {
        unsafe { sys::lkDcGetBufferedAmount(self.ffi.as_ptr()) }
    }

    pub fn on_state_change(&self, handler: Option<OnStateChange>) {
        let mut guard = self.observer.state_change_handler.lock().unwrap();
        guard.replace(handler.unwrap());
    }

    pub fn on_message(&self, handler: Option<OnMessage>) {
        let mut guard = self.observer.message_handler.lock().unwrap();
        guard.replace(handler.unwrap());
    }

    pub fn on_buffered_amount_change(&self, handler: Option<OnBufferedAmountChange>) {
        let mut guard = self.observer.buffered_amount_change_handler.lock().unwrap();
        guard.replace(handler.unwrap());
    }
}

impl Debug for DataChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataChannel")
            .field("id", &self.id())
            .field("label", &self.label())
            .field("state", &self.state())
            .finish()
    }
}

#[derive(Default)]
pub struct DataChannelObserver {
    state_change_handler: Mutex<Option<OnStateChange>>,
    message_handler: Mutex<Option<OnMessage>>,
    buffered_amount_change_handler: Mutex<Option<OnBufferedAmountChange>>,
}

impl DataChannelObserver {
    pub extern "C" fn on_state_change(userdata: *mut ::std::os::raw::c_void, state: lkDcState) {
        println!(
            "DataChannelObserver::on_state_change called with state: {:?}, id {:?}",
            state, userdata
        );

        let observer = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.state_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    pub extern "C" fn on_message(
        data: *const u8,
        size: u64,
        binary: bool,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        println!("DataChannelObserver::on_message called with size: {}, binary {}", size, binary);
        let observer: &DataChannelObserver = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.message_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            let data_slice = unsafe { std::slice::from_raw_parts(data, size as usize) };
            f(DataBuffer { data: data_slice, binary });
        }
    }

    pub extern "C" fn on_buffered_amount_change(
        sent_data_size: u64,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        println!(
            "DataChannelObserver::on_buffered_amount_change called with size: {}",
            sent_data_size
        );

        let observer = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.buffered_amount_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(sent_data_size);
        }
    }
}
