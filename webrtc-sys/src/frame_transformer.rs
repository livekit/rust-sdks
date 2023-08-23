use crate::impl_thread_safety;
use cxx::UniquePtr;
use crate::encoded_video_frame::ffi::EncodedVideoFrame;
use crate::encoded_audio_frame::ffi::EncodedAudioFrame;
use crate::sender_report::ffi::SenderReport;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    extern "C++" {
        include!("livekit/encoded_video_frame.h");
        include!("livekit/encoded_audio_frame.h");
        include!("livekit/sender_report.h");

        type EncodedVideoFrame = crate::encoded_video_frame::ffi::EncodedVideoFrame;
        type EncodedAudioFrame = crate::encoded_audio_frame::ffi::EncodedAudioFrame;

        type SenderReport = crate::sender_report::ffi::SenderReport;
    }

    unsafe extern "C++" {
        include!("livekit/frame_transformer.h");
        include!("livekit/encoded_video_frame.h");
        include!("livekit/encoded_audio_frame.h");
        include!("livekit/sender_report.h");

        type AdaptedNativeFrameTransformer;
        type AdaptedNativeSenderReportCallback;

        fn new_adapted_frame_transformer(
            observer: Box<EncodedFrameSinkWrapper>,
            is_video: bool
        ) -> SharedPtr<AdaptedNativeFrameTransformer>;

        fn new_adapted_sender_report_callback(
            observer: Box<SenderReportSinkWrapper>
        ) -> SharedPtr<AdaptedNativeSenderReportCallback>;
    }

    unsafe extern "C++" {
        
    }

    extern "Rust" {
        type EncodedFrameSinkWrapper;
        type SenderReportSinkWrapper;

        fn on_encoded_video_frame(self: &EncodedFrameSinkWrapper, frame: UniquePtr<EncodedVideoFrame>);
        fn on_encoded_audio_frame(self: &EncodedFrameSinkWrapper, frame: UniquePtr<EncodedAudioFrame>);

        fn on_sender_report(self: &SenderReportSinkWrapper, sender_report: UniquePtr<SenderReport>);
    }
}

impl_thread_safety!(ffi::AdaptedNativeFrameTransformer, Send + Sync);
impl_thread_safety!(ffi::AdaptedNativeSenderReportCallback, Send + Sync);

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

pub trait SenderReportSink: Send {
    fn on_sender_report(&self, sender_report: UniquePtr<SenderReport>);
}

pub struct SenderReportSinkWrapper {
    observer: *mut dyn SenderReportSink,
}

impl SenderReportSinkWrapper {
    /// # Safety
    /// SenderReportSink must live as long as SenderReportSinkWrapper does
    pub unsafe fn new(observer: *mut dyn SenderReportSink) -> Self {
        Self { observer }
    }

    fn on_sender_report(&self, sender_report: UniquePtr<SenderReport>) {
        println!("SenderReportSinkWrapper::on_sender_report");
        unsafe {
            (*self.observer).on_sender_report(sender_report);
        }
    }
}
