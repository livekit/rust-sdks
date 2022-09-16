use cxx::UniquePtr;

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
    //#[allow(deprecated)]
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
        priority: Priority
    }

    unsafe extern "C++" {
        include!("livekit/data_channel.h");

        type DataChannel;
        type NativeDataChannelInit;

        fn create_data_channel_init(init: DataChannelInit) -> UniquePtr<NativeDataChannelInit>;

        fn _unique_data_channel() -> UniquePtr<DataChannel>; // Ignore
    }
}
