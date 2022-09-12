use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/data_channel.h");

        type DataChannel;

        fn _unique_data_channel() -> UniquePtr<DataChannel>; // Ignore
    }
}
