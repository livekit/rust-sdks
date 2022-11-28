//
// Created by theom on 14/11/2022.
//

#ifndef LIVEKIT_WEBRTC_VIDEO_FRAME_H
#define LIVEKIT_WEBRTC_VIDEO_FRAME_H

#include "api/video/video_frame.h"
#include "livekit/rust_types.h"
#include "livekit/video_frame_buffer.h"

namespace livekit {

class VideoFrame {
 public:
  explicit VideoFrame(const webrtc::VideoFrame& frame)
      : frame_(std::move(frame)) {}

  int width() const { return frame_.width(); }
  int height() const { return frame_.height(); }
  uint32_t size() const { return frame_.size(); }
  uint16_t id() const { return frame_.id(); }
  int64_t timestamp_us() const { return frame_.timestamp_us(); }
  int64_t ntp_time_ms() const { return frame_.ntp_time_ms(); }
  uint32_t transport_frame_id() const { return frame_.transport_frame_id(); }
  uint32_t timestamp() const { return frame_.timestamp(); }

  VideoRotation rotation() const {
    return static_cast<VideoRotation>(frame_.rotation());
  }

  // TODO(theomonnom) This shouldn't create a new shared_ptr at each call
  std::unique_ptr<VideoFrameBuffer> video_frame_buffer() const {
    return std::make_unique<VideoFrameBuffer>(frame_.video_frame_buffer());
  }

 private:
  webrtc::VideoFrame frame_;
};

static std::unique_ptr<VideoFrame> _unique_video_frame() {
  return nullptr;  // Ignore
}


}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_VIDEO_FRAME_H
