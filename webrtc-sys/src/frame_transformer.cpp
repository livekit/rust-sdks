#include "livekit/frame_transformer.h"

namespace livekit {

// will receive RtpReceiverObserver 
NativeFrameTransformer::NativeFrameTransformer() {
}


void NativeFrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {

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

// FrameTransformerInterface::FrameTransformerInterface(
//     rtc::scoped_refptr<FrameTransformer> transformer)
//     : transformer_(transformer) {}

AdaptedNativeFrameTransformer::AdaptedNativeFrameTransformer(
    rtc::scoped_refptr<NativeFrameTransformer> source)
    : source_(source) {}

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

