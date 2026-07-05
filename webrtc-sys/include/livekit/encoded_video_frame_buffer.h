/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>

#include "api/video/encoded_image.h"
#include "api/video/video_frame_buffer.h"

namespace livekit {

enum class EncodedVideoCodec {
  kH264,
  kH265,
  kVP8,
  kVP9,
  kAV1,
};

enum class EncodedFrameType {
  kKey,
  kDelta,
};

struct EncodedRateControlRequest {
  bool has_request = false;
  uint64_t target_bitrate_bps = 0;
  double framerate_fps = 0.0;
};

// Latest-wins rate-control mailbox shared between the pass-through encoder and
// the Rust capture side.
class EncodedRateControlState {
 public:
  void Store(uint64_t target_bitrate_bps, double framerate_fps);
  EncodedRateControlRequest Take();

 private:
  std::mutex mutex_;
  EncodedRateControlRequest request_;
};

// A native WebRTC frame buffer carrying one encoded video access unit.
class EncodedVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  // `keyframe_request_flag` is shared with the owning video source: the
  // pass-through encoder sets it when the RTP layer asks for a keyframe the
  // pending frame cannot satisfy, and the capture side polls it to forward
  // the request upstream.
  EncodedVideoFrameBuffer(
      int width,
      int height,
      EncodedVideoCodec codec,
      EncodedFrameType frame_type,
      webrtc::scoped_refptr<webrtc::EncodedImageBuffer> payload,
      std::shared_ptr<std::atomic<bool>> keyframe_request_flag = nullptr,
      std::shared_ptr<EncodedRateControlState> rate_control_state = nullptr);
  ~EncodedVideoFrameBuffer() override = default;

  Type type() const override;
  int width() const override;
  int height() const override;
  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> CropAndScale(
      int offset_x,
      int offset_y,
      int crop_width,
      int crop_height,
      int scaled_width,
      int scaled_height) override;

  EncodedVideoCodec codec() const { return codec_; }
  EncodedFrameType frame_type() const { return frame_type_; }

  // The encoded access unit. Shared with the pass-through encoder so the
  // payload is not copied again on the send path.
  webrtc::scoped_refptr<webrtc::EncodedImageBuffer> encoded_data() const {
    return payload_;
  }
  const uint8_t* payload_data() const { return payload_->data(); }
  size_t payload_size() const { return payload_->size(); }

  // Asks the capture side to produce a keyframe (e.g. on PLI/FIR).
  void request_keyframe() const;

  // Updates the capture side with the latest encoder rate-control target.
  void set_rate_control_request(uint64_t target_bitrate_bps,
                                double framerate_fps) const;

  static EncodedVideoFrameBuffer* FromNative(webrtc::VideoFrameBuffer* buffer);

 private:
  int width_;
  int height_;
  EncodedVideoCodec codec_;
  EncodedFrameType frame_type_;
  webrtc::scoped_refptr<webrtc::EncodedImageBuffer> payload_;
  std::shared_ptr<std::atomic<bool>> keyframe_request_flag_;
  std::shared_ptr<EncodedRateControlState> rate_control_state_;
};

}  // namespace livekit
