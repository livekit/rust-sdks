#pragma once

#include "livekit/frame_transformer.h"
#include "rtc_base/checks.h"

namespace livekit {
class EncodedAudioFrame;
}  // namespace livekit
#include "webrtc-sys/src/encoded_audio_frame.rs.h"

namespace livekit {

class EncodedAudioFrame {
 public:
  explicit EncodedAudioFrame(std::unique_ptr<webrtc::TransformableAudioFrameInterface> frame);

  uint32_t timestamp() const;

  uint8_t payload_type() const;
  const uint8_t* payload_data() const;
  size_t payload_size() const;

 private:
  std::unique_ptr<webrtc::TransformableAudioFrameInterface> frame_;
  const uint8_t* data;
  size_t size;
};

}  // namespace livekit