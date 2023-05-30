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

    unsafe extern "C++" {
        include!("livekit/data_channel.h");

        type DataChannel;

        fn register_observer(self: &DataChannel, observer: Box<BoxDataChannelObserver>);
        fn unregister_observer(self: &DataChannel);

        fn send(self: &DataChannel, data: &DataBuffer) -> bool;
        fn label(self: &DataChannel) -> String;
        fn state(self: &DataChannel) -> DataState;
        fn close(self: &DataChannel);

        fn _shared_data_channel() -> SharedPtr<DataChannel>; // Ignore
    }

    extern "Rust" {
        type BoxDataChannelObserver;

        fn on_state_change(self: &BoxDataChannelObserver, state: DataState);
        fn on_message(self: &BoxDataChannelObserver, buffer: DataBuffer);
        fn on_buffered_amount_change(self: &BoxDataChannelObserver, sent_data_size: u64);
    }
}

impl_thread_safety!(ffi::DataChannel, Send + Sync);

pub trait DataChannelObserver: Send + Sync {
    fn on_state_change(&self, state: ffi::DataState);
    fn on_message(&self, data: &[u8], is_binary: bool);
    fn on_buffered_amount_change(&self, sent_data_size: u64);
}

type BoxDataChannelObserver = Box<dyn DataChannelObserver>;
