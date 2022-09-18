use cxx::UniquePtr;
use std::slice;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
    #[repr(u32)]
    enum Priority {
        VeryLow,
        Low,
        Medium,
        High,
    }

    #[derive(Debug)]
    #[allow(deprecated)]
    pub struct DataChannelInit {
        #[deprecated]
        reliable: bool,
        ordered: bool,
        has_max_retransmit_time: bool,
        max_retransmit_time: i32,
        has_max_retransmits: bool,
        max_retransmits: i32,
        protocol: String,
        negotiated: bool,
        id: i32,
        has_priority: bool,
        priority: Priority,
    }

    #[derive(Debug)]
    pub struct DataBuffer {
        pub ptr: *const u8,
        pub len: usize,
        pub binary: bool,
    }

    #[derive(Debug)]
    pub enum DataState {
        Connecting,
        Open,
        Closing,
        Closed,
    }

    extern "Rust" {
        type DataChannelObserverWrapper;

        fn on_state_change(self: &DataChannelObserverWrapper);
        fn on_message(self: &DataChannelObserverWrapper, buffer: DataBuffer);
        fn on_buffered_amount_change(self: &DataChannelObserverWrapper, sent_data_size: u64);
    }

    unsafe extern "C++" {
        include!("livekit/data_channel.h");

        type DataChannel;
        type NativeDataChannelInit;
        type NativeDataChannelObserver;

        fn close(self: Pin<&mut DataChannel>);

        fn create_data_channel_init(init: DataChannelInit) -> UniquePtr<NativeDataChannelInit>;
        fn create_native_data_channel_observer(
            observer: Box<DataChannelObserverWrapper>,
        ) -> UniquePtr<NativeDataChannelObserver>;

        fn _unique_data_channel() -> UniquePtr<DataChannel>; // Ignore
    }
}

// DataChannelObserver

pub trait DataChannelObserver: Send {
    fn on_state_change(&self);
    fn on_message(&self, data: &[u8], is_binary: bool);
    fn on_buffered_amount_change(&self, sent_data_size: u64);
}

pub struct DataChannelObserverWrapper {
    observer: Box<dyn DataChannelObserver>,
}

impl DataChannelObserverWrapper {
    pub fn new(observer: Box<dyn DataChannelObserver>) -> Self {
        Self { observer }
    }

    fn on_state_change(&self) {
        self.observer.on_state_change();
    }

    fn on_message(&self, buffer: ffi::DataBuffer) {
        let data = unsafe { slice::from_raw_parts(buffer.ptr, buffer.len) };
        self.observer.on_message(data, buffer.binary);
    }

    fn on_buffered_amount_change(&self, sent_data_size: u64) {
        self.observer.on_buffered_amount_change(sent_data_size);
    }
}
