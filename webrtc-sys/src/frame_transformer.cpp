#include "livekit/frame_transformer.h"

namespace livekit {

void FrameTransformer::Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame) {

}

FrameTransformerInterface::FrameTransformerInterface(
    rtc::scoped_refptr<FrameTransformer> transformer)
    : transformer_(transformer) {}

void new_frame_transformer(
    //rust::Box<VideoFrameSinkWrapper> observer
    ) {
    fprintf(stderr, "new_frame_transformer()");
}

}

