#include "livekit/frame_transformer.h"
#include "livekit/encoded_video_frame.h"

namespace livekit {

NativeFrameTransformer::NativeFrameTransformer(
    rust::Box<EncodedFrameSinkWrapper> observer, bool is_video) : observer_(std::move(observer)), is_video(is_video) {
}

void NativeFrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {
    // fprintf(stderr, "NativeFrameTransformer::Transform\n");
    if (is_video) {
        // fprintf(stderr, "Video\n");
        std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame(static_cast<webrtc::TransformableVideoFrameInterface*>(transformable_frame.release()));
        // fprintf(stderr, "TransformableVideoFrameInterface is keyframe? %d\n", frame->IsKeyFrame());
        observer_->on_encoded_video_frame(std::make_unique<EncodedVideoFrame>(std::move(frame)));
    }
    else {
        // fprintf(stderr, "Audio\n");
        std::unique_ptr<webrtc::TransformableAudioFrameInterface> frame(static_cast<webrtc::TransformableAudioFrameInterface*>(transformable_frame.release()));
        observer_->on_encoded_audio_frame(std::make_unique<EncodedAudioFrame>(std::move(frame)));
    }
}

AdaptedNativeFrameTransformer::AdaptedNativeFrameTransformer(
    rtc::scoped_refptr<NativeFrameTransformer> source)
    : source_(source) {}

rtc::scoped_refptr<NativeFrameTransformer> AdaptedNativeFrameTransformer::get()
    const {
  return source_;
}

std::shared_ptr<AdaptedNativeFrameTransformer> new_adapted_frame_transformer(
    rust::Box<EncodedFrameSinkWrapper> observer,
    bool is_video
    ) {
    // fprintf(stderr, "new_adapted_frame_transformer()\n");
    
    return std::make_shared<AdaptedNativeFrameTransformer>(
        rtc::scoped_refptr<NativeFrameTransformer>(new NativeFrameTransformer(std::move(observer), is_video))
    );
}

// sender report 

NativeSenderReportCallback::NativeSenderReportCallback(
    rust::Box<SenderReportSinkWrapper> observer) : observer_(std::move(observer)) {
}

void NativeSenderReportCallback::OnSenderReport(std::unique_ptr<webrtc::LTSenderReport> sender_report) {
    fprintf(stderr, "NativeSenderReportCallback::OnSenderReport\n");
    observer_->on_sender_report(std::make_unique<SenderReport>(std::move(sender_report)));
}

AdaptedNativeSenderReportCallback::AdaptedNativeSenderReportCallback(
    rtc::scoped_refptr<NativeSenderReportCallback> source)
    : source_(source) {}

rtc::scoped_refptr<NativeSenderReportCallback> AdaptedNativeSenderReportCallback::get()
    const {
  return source_;
}

std::shared_ptr<AdaptedNativeSenderReportCallback> new_adapted_sender_report_callback(
    rust::Box<SenderReportSinkWrapper> observer
    ) {
    fprintf(stderr, "new_adapted_sender_report_callback()\n");
    
    return std::make_shared<AdaptedNativeSenderReportCallback>(
        rtc::scoped_refptr<NativeSenderReportCallback>(new NativeSenderReportCallback(std::move(observer)))
    );
}

}