#pragma once

#include "livekit/frame_transformer.h"
#include "api/video/video_frame_metadata.h"
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

  uint16_t first_seq_num() const;
  uint16_t last_seq_num() const;
  int64_t get_ntp_time_ms() const;

  uint32_t timestamp() const;

  uint8_t payload_type() const;
  std::shared_ptr<int64_t> frame_id() const;
  const uint8_t* payload_data() const;
  size_t payload_size() const;

  int temporal_index() const;
  
  uint32_t ssrc() const;

  std::shared_ptr<uint64_t> absolute_capture_timestamp() const;
  std::shared_ptr<int64_t> estimated_capture_clock_offset() const;

  std::unique_ptr<webrtc::TransformableVideoFrameInterface> get_raw_frame();

 private:
  std::unique_ptr<webrtc::TransformableVideoFrameInterface> frame_;
  const uint8_t* data;
  size_t size;
};

}  // namespace livekit