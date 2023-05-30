// use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
//     unsafe extern "C++" {
//         include!("livekit/encoded_frame.h");

//         type EncodedFrame;
//         // fn new_encoded_frame() -> UniquePtr<EncodedFrame>;
//     }

//     impl UniquePtr<EncodedFrame> {}
}

// impl_thread_safety!(ffi::EncodedFrame, Send + Sync);