#include "livekit/frame_transformer.h"
#include "livekit/encoded_video_frame.h"

namespace livekit {

NativeFrameTransformer::NativeFrameTransformer(
    rust::Box<EncodedFrameSinkWrapper> observer, bool is_video) : observer_(std::move(observer)), is_video(is_video) {
}

void NativeFrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {
    // fprintf(stderr, "NativeFrameTransformer::Transform\n");
    if (is_video) {
        std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame(static_cast<webrtc::TransformableVideoFrameInterface*>(transformable_frame.release()));
        observer_->on_encoded_video_frame(std::make_unique<EncodedVideoFrame>(std::move(frame)));
    }
    else {
        std::unique_ptr<webrtc::TransformableAudioFrameInterface> frame(static_cast<webrtc::TransformableAudioFrameInterface*>(transformable_frame.release()));
        observer_->on_encoded_audio_frame(std::make_unique<EncodedAudioFrame>(std::move(frame)));
    }
}

void NativeFrameTransformer::RegisterTransformedFrameCallback(rtc::scoped_refptr<webrtc::TransformedFrameCallback> send_frame_to_sink_callback) {
    fprintf(stderr, "NativeFrameTransformer::RegisterTransformedFrameCallback\n");
    std::lock_guard<std::mutex> guard(sink_mutex_);
    sink_callback_ = send_frame_to_sink_callback;
}

void NativeFrameTransformer::UnregisterTransformedFrameCallback() {
    fprintf(stderr, "NativeFrameTransformer::UnregisterTransformedFrameCallback\n");
    std::lock_guard<std::mutex> guard(sink_mutex_);
    sink_callback_ = nullptr;
}

void NativeFrameTransformer::RegisterTransformedFrameSinkCallback(
      rtc::scoped_refptr<webrtc::TransformedFrameCallback> send_frame_to_sink_callback,
      uint32_t ssrc) {
    fprintf(stderr, "NativeFrameTransformer::RegisterTransformedFrameSinkCallback\n");

    if (send_frame_to_sink_callback == nullptr) {
        fprintf(stderr, "callback is nullptr\n");
    }
    
    std::lock_guard<std::mutex> guard(sink_mutex_);
    sink_callbacks_[ssrc] = send_frame_to_sink_callback;
}

void NativeFrameTransformer::UnregisterTransformedFrameSinkCallback(uint32_t ssrc) {
    fprintf(stderr, "NativeFrameTransformer::UnregisterTransformedFrameSinkCallback\n");

    std::lock_guard<std::mutex> guard(sink_mutex_);
    auto it = sink_callbacks_.find(ssrc);
    if (it != sink_callbacks_.end()) {
      sink_callbacks_.erase(it);
    }
}

void NativeFrameTransformer::FrameTransformed(std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
    // fprintf(stderr, "NativeFrameTransformer::FrameTransformed\n");

    rtc::scoped_refptr<webrtc::TransformedFrameCallback> sink_callback = nullptr;
    {
        std::lock_guard<std::mutex> guard(sink_mutex_);
        sink_callback = sink_callbacks_[frame->GetSsrc()];
        
        if (sink_callback == nullptr) {
            sink_callback = sink_callback_;
        }
    }

    if (sink_callback != nullptr) {
        sink_callback_->OnTransformedFrame(std::move(frame));
    }
    else {
        fprintf(stderr, "did not find callback: %x\n");
    }
}

AdaptedNativeFrameTransformer::AdaptedNativeFrameTransformer(
    rtc::scoped_refptr<NativeFrameTransformer> source)
    : source_(source) {}

rtc::scoped_refptr<NativeFrameTransformer> AdaptedNativeFrameTransformer::get()
    const {
  return source_;
}

void AdaptedNativeFrameTransformer::AudioFrameTransformed(std::unique_ptr<EncodedAudioFrame> frame) const {
    // fprintf(stderr, "AdaptedNativeFrameTransformer::FrameTransformed (audio)\n");
    source_->FrameTransformed(std::move(frame->get_raw_frame()));
}

void AdaptedNativeFrameTransformer::VideoFrameTransformed(std::unique_ptr<EncodedVideoFrame> frame) const {
    // fprintf(stderr, "AdaptedNativeFrameTransformer::FrameTransformed (video)\n");
    source_->FrameTransformed(std::move(frame->get_raw_frame()));
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