#include "livekit/encoded_video_frame.h"

namespace livekit {

EncodedVideoFrame::EncodedVideoFrame(
    std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame) 
    : frame_(std::move(frame)) {
}

bool EncodedVideoFrame::is_key_frame() const {
    return frame_->IsKeyFrame();
}

}