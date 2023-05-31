#include "livekit/frame_transformer.h"

namespace livekit {

NativeFrameTransformer::NativeFrameTransformer(
    rust::Box<EncodedFrameSinkWrapper> observer) : observer_(std::move(observer)) {
} 

void NativeFrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {
    fprintf(stderr, "NativeFrameTransformer::Transform\n");
    std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame(static_cast<webrtc::TransformableVideoFrameInterface*>(transformable_frame.release()));
    fprintf(stderr, "TransformableVideoFrameInterface is keyframe? %d\n", frame->IsKeyFrame());
    observer_->on_encoded_frame();
}

AdaptedNativeFrameTransformer::AdaptedNativeFrameTransformer(
    rtc::scoped_refptr<NativeFrameTransformer> source)
    : source_(source) {}

rtc::scoped_refptr<NativeFrameTransformer> AdaptedNativeFrameTransformer::get()
    const {
  return source_;
}

std::shared_ptr<AdaptedNativeFrameTransformer> new_adapted_frame_transformer(rust::Box<EncodedFrameSinkWrapper> observer) {
    fprintf(stderr, "new_adapted_frame_transformer()\n");
    
    return std::make_shared<AdaptedNativeFrameTransformer>(
        rtc::scoped_refptr<NativeFrameTransformer>(new NativeFrameTransformer(std::move(observer)))
    );
}

}