#pragma once

#include "livekit/frame_transformer.h"
#include "rtc_base/checks.h"

namespace livekit {
class EncodedVideoFrame;
}  // namespace livekit
#include "webrtc-sys/src/encoded_video_frame.rs.h"

namespace livekit {

class EncodedVideoFrame {
 public:
  explicit EncodedVideoFrame(std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame);

  bool is_key_frame() const;
  uint16_t width() const;
  uint16_t height() const;
  uint32_t timestamp() const;

  uint8_t payload_type() const;
  const uint8_t* payload_data() const;
  size_t payload_size() const;

 private:
  std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame_;
  const uint8_t* data;
  size_t size;
};

}  // namespace livekit