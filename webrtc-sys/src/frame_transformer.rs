use crate::impl_thread_safety;
use cxx::UniquePtr;
use crate::encoded_video_frame::ffi::EncodedVideoFrame;
use crate::encoded_audio_frame::ffi::EncodedAudioFrame;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
        include!("livekit/encoded_video_frame.h");
        include!("livekit/encoded_audio_frame.h");

        type EncodedVideoFrame = crate::encoded_video_frame::ffi::EncodedVideoFrame;
        type EncodedAudioFrame = crate::encoded_audio_frame::ffi::EncodedAudioFrame;
    }

    unsafe extern "C++" {
        include!("livekit/frame_transformer.h");
        include!("livekit/encoded_video_frame.h");

        type AdaptedNativeFrameTransformer;
        

        fn new_adapted_frame_transformer(
            observer: Box<EncodedFrameSinkWrapper>,
        // );
        ) -> SharedPtr<AdaptedNativeFrameTransformer>;
    }

    unsafe extern "C++" {
        
    }

    extern "Rust" {
        type EncodedFrameSinkWrapper;

        fn on_encoded_video_frame(self: &EncodedFrameSinkWrapper, frame: UniquePtr<EncodedVideoFrame>);
        fn on_encoded_audio_frame(self: &EncodedFrameSinkWrapper, frame: UniquePtr<EncodedAudioFrame>);
    }
}

impl_thread_safety!(ffi::AdaptedNativeFrameTransformer, Send + Sync);

pub trait EncodedFrameSink: Send {
    // fn on_frame(&self, frame: UniquePtr<EncodedFrame>);
    fn on_encoded_video_frame(&self, frame: UniquePtr<EncodedVideoFrame>);
    fn on_encoded_audio_frame(&self, frame: UniquePtr<EncodedAudioFrame>);
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

    fn on_encoded_video_frame(&self, frame: UniquePtr<EncodedVideoFrame>) {
        // println!("EncodedFrameSinkWrapper::on_frame");
        unsafe {
            (*self.observer).on_encoded_video_frame(frame);
        }
    }

    fn on_encoded_audio_frame(&self, frame: UniquePtr<EncodedAudioFrame>) {
        // println!("EncodedFrameSinkWrapper::on_frame");
        unsafe {
            (*self.observer).on_encoded_audio_frame(frame);
        }
    }
}
