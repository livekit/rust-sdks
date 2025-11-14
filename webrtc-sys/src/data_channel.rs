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

use serde::Deserialize;
use std::{
    str::{self, Utf8Error},
    sync::{Arc, Mutex},
};
use thiserror::Error;

use crate::sys::{self, lkDataChannelObserver, lkDcState};

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
    pub max_retransmit_time: Option<i32>,
    pub max_retransmits: Option<i32>,
    pub protocol: String,
    pub negotiated: bool,
    pub id: i32,
    pub priority: Option<Priority>,
}

impl From<lkDcState> for DataChannelState {
    fn from(value: lkDcState) -> Self {
        match value {
            lkDcState::LK_DC_STATE_CONNECTING => Self::Connecting,
            lkDcState::LK_DC_STATE_OPEN => Self::Open,
            lkDcState::LK_DC_STATE_CLOSING => Self::Closing,
            lkDcState::LK_DC_STATE_CLOSED => Self::Closed,
            _ => panic!("unknown data channel state"),
        }
    }
}

impl From<DataChannelInit> for sys::lkDataChannelInit {
    fn from(value: DataChannelInit) -> Self {
        //TODO: complete conversion
        sys::lkDataChannelInit {
            ordered: value.ordered,
            maxRetransmits: value.max_retransmits.unwrap_or(-1),
            reliable: todo!(),
        }
    }
}

#[derive(Clone)]
pub struct DataChannel {
    observer: Arc<DataChannelObserver>,
    pub(crate) sys_handle: sys::RefCounted<sys::lkDataChannel>,
}

impl DataChannel {
    fn set_observer(&mut self, observer: Arc<DataChannelObserver>) {
        self.observer = observer;
    }
    pub fn configure(sys_handle: sys::RefCounted<sys::lkDataChannel>) -> Self {
        let observer = Arc::new(DataChannelObserver::default());
        let dc: DataChannel = Self { sys_handle: sys_handle.clone(), observer: observer };
        let lk_observer = dc.observer.lk_observer();
        unsafe {
            sys::lkDcRegisterObserver(
                sys_handle.as_ptr(),
                &lk_observer,
                /*TODO: */ core::ptr::null_mut(),
            );
        }
        dc
    }

    extern "C" fn lk_on_complete(
        error: *mut sys::lkRtcError,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let cb: Box<Box<dyn FnOnce(Result<(), DataChannelError>)>> =
            unsafe { Box::from_raw(userdata as *mut _) };
        if error.is_null() {
            cb(Ok(()));
        } else {
            cb(Err(DataChannelError::Send));
        }
    }

    pub fn send(&self, data: &[u8], binary: bool) -> Result<(), DataChannelError> {
        if !binary {
            str::from_utf8(data)?;
        }

        let cb: Box<Box<dyn FnOnce(Result<(), DataChannelError>)>> = Box::new(Box::new(|res| {
            if let Err(err) = res {
                eprintln!("DataChannel send error: {:?}", err);
            }
        }));

        unsafe {
            sys::lkDcSendAsync(
                self.sys_handle.as_ptr(),
                data.as_ptr() as *const u8,
                data.len() as u64,
                binary,
                Some(DataChannel::lk_on_complete),
                Box::into_raw(cb) as *mut ::std::os::raw::c_void,
            );
        }
        //TODO:
        Ok(())
    }

    pub fn id(&self) -> i32 {
        unsafe { sys::lkDcGetId(self.sys_handle.as_ptr()) }
    }

    pub fn label(&self) -> String {
        unsafe {
            let buffer_size = 512;
            let mut buffer: Vec<u8> = Vec::with_capacity(buffer_size as usize);
            buffer.resize(buffer_size as usize, 0);
            sys::lkDcGetLabel(
                self.sys_handle.as_ptr(),
                buffer.as_mut_ptr() as *mut i8,
                buffer_size,
            );
            let rust_str =
                String::from_utf8_lossy(&buffer[..(buffer_size - 1) as usize]).to_string();
            rust_str
        }
    }

    pub fn state(&self) -> DataChannelState {
        let state = unsafe { sys::lkDcGetState(self.sys_handle.as_ptr()) };
        state.into()
    }

    pub fn close(&self) {
        unsafe { sys::lkDcClose(self.sys_handle.as_ptr()) };
    }

    pub fn buffered_amount(&self) -> u64 {
        unsafe { sys::lkDcGetBufferedAmount(self.sys_handle.as_ptr()) }
    }
}

#[derive(Default)]
struct DataChannelObserver {
    dc: Mutex<Option<DataChannel>>,
    state_change_handler: Mutex<Option<OnStateChange>>,
    message_handler: Mutex<Option<OnMessage>>,
    buffered_amount_change_handler: Mutex<Option<OnBufferedAmountChange>>,
}

impl DataChannelObserver {
    fn lk_observer(&self) -> lkDataChannelObserver {
        lkDataChannelObserver {
            onStateChange: Some(DataChannelObserver::lk_on_state_change),
            onMessage: Some(DataChannelObserver::lk_on_message),
            onBufferedAmountChange: Some(DataChannelObserver::lk_on_buffered_amount_change),
        }
    }

    extern "C" fn lk_on_state_change(userdata: *mut ::std::os::raw::c_void, state: lkDcState) {
        let observer = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.state_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(state.into());
        }
    }

    extern "C" fn lk_on_message(
        data: *const u8,
        size: u64,
        binary: bool,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.message_handler.lock().unwrap();

        if let Some(f) = handler.as_mut() {
            let data_slice = unsafe { std::slice::from_raw_parts(data, size as usize) };
            f(DataBuffer { data: data_slice, binary });
        }
    }

    extern "C" fn lk_on_buffered_amount_change(
        sent_data_size: u64,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer = unsafe { &*(userdata as *const DataChannelObserver) };
        let mut handler = observer.buffered_amount_change_handler.lock().unwrap();
        if let Some(f) = handler.as_mut() {
            f(sent_data_size);
        }
    }
}
