/*
 * Copyright 2025 LiveKit, Inc.
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

#include "livekit/video_frame.h"

#include <chrono>
#include <memory>
#include <optional>

#include "api/video/video_frame.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {
namespace {

constexpr int64_t kMaxDecodeTimestampAgeUs = 30'000'000;

uint64_t MonotonicToUnixTimeMicros(webrtc::Timestamp timestamp) {
  if (!timestamp.IsFinite()) {
    return 0;
  }

  const int64_t now_monotonic_us = webrtc::TimeMicros();
  const int64_t timestamp_monotonic_us = timestamp.us();
  if (timestamp_monotonic_us > now_monotonic_us) {
    return 0;
  }

  const int64_t age_us = now_monotonic_us - timestamp_monotonic_us;
  if (age_us > kMaxDecodeTimestampAgeUs) {
    return 0;
  }

  const auto now = std::chrono::system_clock::now().time_since_epoch();
  const uint64_t now_unix_us = static_cast<uint64_t>(
      std::chrono::duration_cast<std::chrono::microseconds>(now).count());
  if (static_cast<uint64_t>(age_us) > now_unix_us) {
    return 0;
  }
  return now_unix_us - static_cast<uint64_t>(age_us);
}

}  // namespace

VideoFrame::VideoFrame(const webrtc::VideoFrame& frame)
    : frame_(std::move(frame)) {}

unsigned int VideoFrame::width() const {
  return frame_.width();
}
unsigned int VideoFrame::height() const {
  return frame_.height();
}
uint32_t VideoFrame::size() const {
  return frame_.size();
}
uint16_t VideoFrame::id() const {
  return frame_.id();
}
int64_t VideoFrame::timestamp_us() const {
  return frame_.timestamp_us();
}
int64_t VideoFrame::ntp_time_ms() const {
  return frame_.ntp_time_ms();
}
uint32_t VideoFrame::timestamp() const {
  return frame_.rtp_timestamp();
}
uint64_t VideoFrame::decode_start_timestamp_us() const {
  const std::optional<webrtc::VideoFrame::ProcessingTime> processing_time =
      frame_.processing_time();
  return processing_time.has_value()
             ? MonotonicToUnixTimeMicros(processing_time->start)
             : 0;
}
uint64_t VideoFrame::decode_finish_timestamp_us() const {
  const std::optional<webrtc::VideoFrame::ProcessingTime> processing_time =
      frame_.processing_time();
  return processing_time.has_value()
             ? MonotonicToUnixTimeMicros(processing_time->finish)
             : 0;
}

VideoRotation VideoFrame::rotation() const {
  return static_cast<VideoRotation>(frame_.rotation());
}

// TODO(theomonnom) This shouldn't create a new shared_ptr at each call
std::unique_ptr<VideoFrameBuffer> VideoFrame::video_frame_buffer() const {
  return std::make_unique<VideoFrameBuffer>(frame_.video_frame_buffer());
}

webrtc::VideoFrame VideoFrame::get() const {
  return frame_;
}

void VideoFrameBuilder::set_video_frame_buffer(const VideoFrameBuffer& buffer) {
  builder_.set_video_frame_buffer(buffer.get());  // const & ref_counted
}

void VideoFrameBuilder::set_timestamp_us(int64_t timestamp_us) {
  builder_.set_timestamp_us(timestamp_us);
}

void VideoFrameBuilder::set_rotation(VideoRotation rotation) {
  builder_.set_rotation(static_cast<webrtc::VideoRotation>(rotation));
}

void VideoFrameBuilder::set_id(uint16_t id) {
  builder_.set_id(id);
}

std::unique_ptr<VideoFrame> VideoFrameBuilder::build() {
  return std::make_unique<VideoFrame>(builder_.build());
}

std::unique_ptr<VideoFrameBuilder> new_video_frame_builder() {
  return std::make_unique<VideoFrameBuilder>();
}

}  // namespace livekit_ffi
