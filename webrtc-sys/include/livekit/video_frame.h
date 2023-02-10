//
// Created by theom on 14/11/2022.
//

#ifndef LIVEKIT_WEBRTC_VIDEO_FRAME_H
#define LIVEKIT_WEBRTC_VIDEO_FRAME_H

#include "api/video/video_frame.h"
#include "livekit/rust_types.h"
#include "livekit/video_frame_buffer.h"
#include "rtc_base/checks.h"

namespace livekit {

class VideoFrame {
 public:
  explicit VideoFrame(const webrtc::VideoFrame& frame);

  int width() const;
  int height() const;
  uint32_t size() const;
  uint16_t id() const;
  int64_t timestamp_us() const;
  int64_t ntp_time_ms() const;
  uint32_t transport_frame_id() const;
  uint32_t timestamp() const;

  VideoRotation rotation() const;
  std::unique_ptr<VideoFrameBuffer> video_frame_buffer() const;

  webrtc::VideoFrame get() const;

 private:
  webrtc::VideoFrame frame_;
};

// Allow to create VideoFrames from Rust,
// the builder pattern will be redone in Rust
class VideoFrameBuilder {
 public:
  VideoFrameBuilder() = default;

  // TODO(theomonnom): other setters?
  void set_video_frame_buffer(std::unique_ptr<VideoFrameBuffer> buffer);
  void set_timestamp_us(int64_t timestamp_us);
  void set_rotation(VideoRotation rotation);
  void set_id(uint16_t id);
  std::unique_ptr<VideoFrame> build();

 private:
  webrtc::VideoFrame::Builder builder_;
};

std::unique_ptr<VideoFrameBuilder> create_video_frame_builder();

}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_VIDEO_FRAME_H
