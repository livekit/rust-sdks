#pragma once

#include "api/frame_transformer_interface.h"

namespace livekit {
class EncodedFrame;
}  // namespace livekit
#include "webrtc-sys/src/encoded_frame.rs.h"

// namespace livekit {
    
// class EncodedFrame : public webrtc::TransformableVideoFrameInterface {
// public:
//     EncodedFrame() = default;
//     rtc::ArrayView<const uint8_t> GetData() const;
//     void SetData(rtc::ArrayView<const uint8_t> data);
//     uint8_t GetPayloadType() const;
//     uint32_t GetSsrc() const;
//     uint32_t GetTimestamp() const;
//     webrtc::TransformableFrameInterface::Direction GetDirection() const;
// };

// // std::unique_ptr<EncodedFrame> new_encoded_frame();

// }