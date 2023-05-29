use crate::{impl_thread_safety, encoded_frame::ffi::EncodedFrame};
use cxx::UniquePtr;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/frame_transformer.h");
        type FrameTransformer;
        type FrameTransformerInterface;

        fn new_frame_transformer(
            //observer: Box<VideoFrameSinkWrapper>,
        );
        //fn new_frame_transformer() -> SharedPtr<FrameTransformer>;
    }
}

impl_thread_safety!(ffi::FrameTransformer, Send + Sync);

pub trait EncodedFrameSink: Send {
    fn on_frame(&self, frame: UniquePtr<EncodedFrame>);
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

    fn on_frame(&self, frame: UniquePtr<EncodedFrame>) {
        unsafe {
            (*self.observer).on_frame(frame);
        }
    }
}
