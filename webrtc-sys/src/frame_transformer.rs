use crate::impl_thread_safety;
// use crate::{impl_thread_safety, encoded_frame::ffi::EncodedFrame};
// use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/frame_transformer.h");

        type AdaptedNativeFrameTransformer;

        fn new_adapted_frame_transformer(
            observer: Box<EncodedFrameSinkWrapper>,
        // );
        ) -> SharedPtr<AdaptedNativeFrameTransformer>;
    }

    extern "Rust" {
        type EncodedFrameSinkWrapper;

        fn on_encoded_frame(self: &EncodedFrameSinkWrapper);
    }
}

impl_thread_safety!(ffi::AdaptedNativeFrameTransformer, Send + Sync);

pub trait EncodedFrameSink: Send {
    // fn on_frame(&self, frame: UniquePtr<EncodedFrame>);
    fn on_encoded_frame(&self);
}

pub struct EncodedFrameSinkWrapper {
    observer: *mut dyn EncodedFrameSink,
}

impl EncodedFrameSinkWrapper {
    /// # Safety
    /// EncodedFrameSink must live as long as EncodedFrameSinkWrapper does
    pub unsafe fn new(observer: *mut dyn EncodedFrameSink) -> Self {
        Self { observer }
    }

    // fn on_frame(&self, frame: UniquePtr<EncodedFrame>) {
    fn on_encoded_frame(&self) {
        // println!("EncodedFrameSinkWrapper::on_frame");
        unsafe {
            (*self.observer).on_encoded_frame();
        }
    }

}
