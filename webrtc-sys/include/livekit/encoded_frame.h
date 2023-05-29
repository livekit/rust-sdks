#pragma once

#include "api/frame_transformer_interface.h"

namespace livekit {
class EncodedFrame;
}  // namespace livekit
#include "webrtc-sys/src/encoded_frame.rs.h"


namespace livekit {
class EncodedFrame  {
public:
    //: public webrtc::TransformableFrameInterface
    EncodedFrame() = default;
    // rtc::ArrayView<const uint8_t> GetData();
};

// std::unique_ptr<EncodedFrame> new_encoded_frame();

}