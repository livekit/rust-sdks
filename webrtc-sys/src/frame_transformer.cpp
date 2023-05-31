#include "livekit/frame_transformer.h"

namespace livekit {

// will receive RtpReceiverObserver 
NativeFrameTransformer::NativeFrameTransformer() {
}


void NativeFrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {
    fprintf(stderr, "NativeFrameTransformer::Transform\n");
    std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame(static_cast<webrtc::TransformableVideoFrameInterface*>(transformable_frame.release()));
    fprintf(stderr, "TransformableVideoFrameInterface is keyframe? %d\n", frame->IsKeyFrame());
}

// void FrameTransformer::RegisterTransformedFrameCallback(
//     rtc::scoped_refptr<webrtc::TransformedFrameCallback>) {

// }

// void FrameTransformer::RegisterTransformedFrameSinkCallback(
//     rtc::scoped_refptr<webrtc::TransformedFrameCallback>,
//     uint32_t ssrc) {

// }

// void FrameTransformer::RegisterTransformedFrameSinkCallback(
//     rtc::scoped_refptr<webrtc::TransformedFrameCallback>,
//     uint32_t ssrc) {

// }

// void FrameTransformer::UnregisterTransformedFrameCallback() {

// }

// void FrameTransformer::UnregisterTransformedFrameSinkCallback(uint32_t ssrc) {

// }

AdaptedNativeFrameTransformer::AdaptedNativeFrameTransformer(
    rtc::scoped_refptr<NativeFrameTransformer> source)
    : source_(source) {}

rtc::scoped_refptr<NativeFrameTransformer> AdaptedNativeFrameTransformer::get()
    const {
  return source_;
}

std::shared_ptr<AdaptedNativeFrameTransformer> new_adapted_frame_transformer() {
    fprintf(stderr, "new_adapted_frame_transformer()\n");
    
    return std::make_shared<AdaptedNativeFrameTransformer>(
        rtc::scoped_refptr<NativeFrameTransformer>(new NativeFrameTransformer())
    );
}

// // std::shared_ptr<FrameTransformer> new_frame_transformer() {
// rtc::scoped_refptr<NativeFrameTransformer> new_frame_transformer() {
// // void new_frame_transformer() {
//     fprintf(stderr, "new_frame_transformer()\n");
    
//     // 
//     // return std::make_shared<FrameTransformer>(
//     //     rtc::make_ref_counted<FrameTransformer>());

//     // return std::make_shared<FrameTransformer>(new FrameTransformer());

//     // return std::make_shared<FrameTransformer>();

//     return rtc::scoped_refptr<NativeFrameTransformer>(new NativeFrameTransformer());
// }

}

