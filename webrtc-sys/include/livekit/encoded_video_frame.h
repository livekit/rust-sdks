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
  void get_data() const;

 private:
  std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame_;
};

}  // namespace livekit