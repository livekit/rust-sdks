use crate::impl_thread_safety;
use std::slice;
use std::sync::Arc;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    #[derive(Debug)]
    #[repr(i32)]
    pub enum Priority {
        VeryLow,
        Low,
        Medium,
        High,
    }

    #[derive(Debug)]
    pub struct DataChannelInit {
        pub ordered: bool,
        pub has_max_retransmit_time: bool,
        pub max_retransmit_time: i32,
        pub has_max_retransmits: bool,
        pub max_retransmits: i32,
        pub protocol: String,
        pub negotiated: bool,
        pub id: i32,
        pub has_priority: bool,
        pub priority: Priority,
    }

    #[derive(Debug)]
    pub struct DataBuffer {
        pub ptr: *const u8,
        pub len: usize,
        pub binary: bool,
    }

    #[derive(Debug)]
    #[repr(i32)]
    pub enum DataState {
        Connecting,
        Open,
        Closing,
        Closed,
    }

    extern "Rust" {
        type DataChannelObserverWrapper;

        fn on_state_change(self: &DataChannelObserverWrapper, state: DataState);
        fn on_message(self: &DataChannelObserverWrapper, buffer: DataBuffer);
        fn on_buffered_amount_change(self: &DataChannelObserverWrapper, sent_data_size: u64);
    }

    unsafe extern "C++" {
        include!("livekit/data_channel.h");

        type DataChannel;
        type NativeDataChannelInit;
        type NativeDataChannelObserver;

        /// SAFETY
        /// The observer must live as long as the datachannel uses it
        unsafe fn register_observer(self: &DataChannel, observer: *mut NativeDataChannelObserver);

        fn unregister_observer(self: &DataChannel);
        fn send(self: &DataChannel, data: &DataBuffer) -> bool;
        fn label(self: &DataChannel) -> String;
        fn state(self: &DataChannel) -> DataState;
        fn close(self: &DataChannel);

        fn create_data_channel_init(init: DataChannelInit) -> UniquePtr<NativeDataChannelInit>;
        unsafe fn create_native_data_channel_observer(
            observer: Box<DataChannelObserverWrapper>,
            dc: *const DataChannel,
        ) -> SharedPtr<NativeDataChannelObserver>;

        fn _shared_data_channel() -> SharedPtr<DataChannel>; // Ignore
    }
}

impl_thread_safety!(ffi::DataChannel, Send + Sync);
impl_thread_safety!(ffi::NativeDataChannelObserver, Send + Sync);

// DataChannelObserver

pub trait DataChannelObserver: Send + Sync {
    fn on_state_change(&self, state: ffi::DataState);
    fn on_message(&self, data: &[u8], is_binary: bool);
    fn on_buffered_amount_change(&self, sent_data_size: u64);
}

pub struct DataChannelObserverWrapper {
    observer: Arc<dyn DataChannelObserver>,
}

impl DataChannelObserverWrapper {
    pub fn new(observer: Arc<dyn DataChannelObserver>) -> Self {
        Self { observer }
    }

    fn on_state_change(&self, state: ffi::DataState) {
        self.observer.on_state_change(state);
    }

    fn on_message(&self, buffer: ffi::DataBuffer) {
        unsafe {
            let data = slice::from_raw_parts(buffer.ptr, buffer.len);
            self.observer.on_message(data, buffer.binary);
        }
    }

    fn on_buffered_amount_change(&self, sent_data_size: u64) {
        self.observer.on_buffered_amount_change(sent_data_size);
    }
}
