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

#include <cstdint>
#include <deque>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "api/environment/environment.h"
#include "api/scoped_refptr.h"
#include "api/video/color_space.h"
#include "api/video/video_codec_type.h"
#include "api/video/video_frame.h"
#include "api/video/video_frame_buffer.h"
#include "api/video/video_rotation.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "common_video/h264/h264_bitstream_parser.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

// Presents a Rust encoder (livekit-hwcodec) to WebRTC. Only what must be C++.

namespace livekit_ffi {

struct FrameInfo;
struct RustEncoderFactory;
struct RustVideoEncoder;

class I420View {
 public:
  explicit I420View(webrtc::scoped_refptr<webrtc::I420BufferInterface> buffer);

  uint32_t width() const;
  uint32_t height() const;
  // Y, U, V.
  rust::Slice<const uint8_t> plane(size_t index) const;
  uint32_t stride(size_t index) const;

 private:
  const webrtc::scoped_refptr<webrtc::I420BufferInterface> buffer_;
};

// Keeps a frame alive for the encoder's thread.
class VideoFrameRef {
 public:
  explicit VideoFrameRef(
      webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer);

  std::unique_ptr<I420View> to_i420() const;

 private:
  const webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};

// Hands the encoder thread's output to WebRTC. Once Detach() returns, no
// callback is running and none will start.
class EncodedSink {
 public:
  EncodedSink(webrtc::EncodedImageCallback* callback,
              webrtc::VideoCodecType codec,
              uint32_t width,
              uint32_t height);

  void SetCallback(webrtc::EncodedImageCallback* callback);
  void Detach();

  // Keeps what the encoded image must carry over from `frame`.
  void Expect(const webrtc::VideoFrame& frame);

  // Called on the encoder's thread. Copies `data`.
  void deliver(rust::Slice<const uint8_t> data,
               const FrameInfo& info,
               bool keyframe) const;
  void dropped(const FrameInfo& info) const;

 private:
  struct Expected {
    uint32_t rtp_timestamp;
    int64_t ntp_time_ms;
    webrtc::VideoRotation rotation;
    std::optional<webrtc::ColorSpace> color_space;
  };
  std::optional<Expected> Take(uint32_t rtp_timestamp) const
      RTC_EXCLUSIVE_LOCKS_REQUIRED(mutex_);

  const webrtc::VideoCodecType codec_;
  const uint32_t width_;
  const uint32_t height_;
  mutable webrtc::Mutex mutex_;
  mutable webrtc::EncodedImageCallback* callback_ RTC_GUARDED_BY(mutex_);
  mutable std::deque<Expected> expected_ RTC_GUARDED_BY(mutex_);
  mutable webrtc::H264BitstreamParser h264_parser_ RTC_GUARDED_BY(mutex_);
};

class RustVideoEncoderFactory final : public webrtc::VideoEncoderFactory {
 public:
  explicit RustVideoEncoderFactory(rust::Box<RustEncoderFactory> factory);
  ~RustVideoEncoderFactory() override;

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;
  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& env,
      const webrtc::SdpVideoFormat& format) override;

 private:
  rust::Box<RustEncoderFactory> factory_;
};

// Null if the system has no V4L2 encoder.
std::unique_ptr<webrtc::VideoEncoderFactory> CreateV4l2VideoEncoderFactory();

// Probes once.
bool HasV4l2VideoEncoder();

#ifdef LIVEKIT_TEST
// Encodes on a missing device until three frames come out. Returns the
// implementation that produced each.
rust::Vec<rust::String> rust_encoder_fallback_for_test();

// Releases, re-initializes or destroys mid-session `cycles` encoders on a
// missing device.
void rust_encoder_lifecycle_for_test(uint32_t cycles);

// Returns how many of two deliveries, either side of Detach(), arrived.
uint32_t encoded_sink_detach_for_test();
#endif

}  // namespace livekit_ffi
