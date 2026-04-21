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

#include "livekit/encoded_video_source.h"

#include <algorithm>
#include <utility>

#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video/video_rotation.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

// ---------- EncodedSourceRegistry ----------

EncodedSourceRegistry& EncodedSourceRegistry::instance() {
  static EncodedSourceRegistry reg;
  return reg;
}

uint16_t EncodedSourceRegistry::allocate_id() {
  std::lock_guard<std::mutex> lock(mu_);
  // Skip kNotSetId (0) and any id currently mapped. With 65535 usable slots
  // and short-lived encoded tracks this loop is effectively O(1).
  for (uint32_t probe = 0; probe < 0x10000u; ++probe) {
    uint16_t candidate = static_cast<uint16_t>(next_id_);
    next_id_ = next_id_ + 1;
    if (next_id_ > 0xFFFFu) {
      next_id_ = 1;
    }
    if (candidate == 0) continue;
    if (map_.find(candidate) == map_.end()) {
      return candidate;
    }
  }
  RTC_LOG(LS_ERROR)
      << "EncodedSourceRegistry exhausted all 65535 slots; reusing 1";
  return 1;
}

void EncodedSourceRegistry::register_source(uint16_t id,
                                            EncodedVideoTrackSource* src) {
  std::lock_guard<std::mutex> lock(mu_);
  map_[id] = src;
}

void EncodedSourceRegistry::unregister_source(uint16_t id) {
  std::lock_guard<std::mutex> lock(mu_);
  map_.erase(id);
}

EncodedVideoTrackSource* EncodedSourceRegistry::lookup(uint16_t id) {
  if (id == 0) return nullptr;
  std::lock_guard<std::mutex> lock(mu_);
  auto it = map_.find(id);
  return it == map_.end() ? nullptr : it->second;
}

// ---------- EncodedVideoTrackSource::InternalSource ----------

EncodedVideoTrackSource::InternalSource::InternalSource(
    uint16_t source_id,
    EncodedVideoCodecType codec,
    uint32_t width,
    uint32_t height)
    : webrtc::AdaptedVideoTrackSource(/*required_alignment=*/1),
      source_id_(source_id),
      codec_(codec),
      width_(width),
      height_(height) {}

EncodedVideoTrackSource::InternalSource::~InternalSource() = default;

bool EncodedVideoTrackSource::InternalSource::push_encoded_frame(
    std::vector<uint8_t> data,
    bool is_keyframe,
    bool has_sps_pps,
    uint32_t width,
    uint32_t height,
    int64_t capture_time_us) {
  {
    webrtc::MutexLock lock(&mutex_);

    if (width != 0 && height != 0) {
      width_ = width;
      height_ = height;
    }

    // Bounded queue: drop-oldest, but never drop a keyframe.
    while (queue_.size() >= kMaxQueueSize) {
      if (queue_.front().is_keyframe && !is_keyframe) {
        RTC_LOG(LS_WARNING)
            << "EncodedVideoTrackSource[" << source_id_
            << "] queue full; dropping incoming delta to preserve keyframe";
        return false;
      }
      queue_.pop_front();
    }

    DequeuedFrame f;
    f.data = std::move(data);
    f.is_keyframe = is_keyframe;
    f.has_sps_pps = has_sps_pps;
    f.width = width_;
    f.height = height_;
    f.capture_time_us = capture_time_us;
    queue_.push_back(std::move(f));
  }

  // Emit a dummy VideoFrame so the WebRTC pipeline ticks. The actual bytes
  // are pulled out by PassthroughVideoEncoder via the registry, keyed on
  // source_id_ stamped into VideoFrame::id().
  //
  // The dummy buffer is 2x2 I420 black; callers never see it. WebRTC needs
  // *some* buffer here. The width/height on the VideoFrame carry the real
  // resolution so downstream stats, pacing, and simulcast decisions work.
  auto dummy_buffer = webrtc::I420Buffer::Create(2, 2);
  webrtc::I420Buffer::SetBlack(dummy_buffer.get());

  webrtc::VideoFrame frame =
      webrtc::VideoFrame::Builder()
          .set_video_frame_buffer(dummy_buffer)
          .set_rotation(webrtc::kVideoRotation_0)
          .set_timestamp_us(capture_time_us != 0 ? capture_time_us
                                                 : webrtc::TimeMicros())
          .set_id(source_id_)
          .build();

  OnFrame(frame);
  return true;
}

bool EncodedVideoTrackSource::InternalSource::pop_encoded_frame(
    DequeuedFrame& out) {
  webrtc::MutexLock lock(&mutex_);
  if (queue_.empty()) return false;
  out = std::move(queue_.front());
  queue_.pop_front();
  return true;
}

void EncodedVideoTrackSource::InternalSource::notify_keyframe_requested() {
  webrtc::MutexLock lock(&mutex_);
  if (observer_) {
    (*observer_)->on_keyframe_requested();
  }
}

void EncodedVideoTrackSource::InternalSource::notify_target_bitrate(
    uint32_t bitrate_bps,
    double framerate_fps) {
  webrtc::MutexLock lock(&mutex_);
  if (observer_) {
    (*observer_)->on_target_bitrate(bitrate_bps, framerate_fps);
  }
}

void EncodedVideoTrackSource::InternalSource::set_observer(
    rust::Box<EncodedVideoSourceWrapper> observer) {
  webrtc::MutexLock lock(&mutex_);
  observer_ = std::make_unique<rust::Box<EncodedVideoSourceWrapper>>(
      std::move(observer));
}

// ---------- EncodedVideoTrackSource ----------

EncodedVideoTrackSource::EncodedVideoTrackSource(EncodedVideoCodecType codec,
                                                 uint32_t width,
                                                 uint32_t height) {
  uint16_t id = EncodedSourceRegistry::instance().allocate_id();
  source_ = webrtc::make_ref_counted<InternalSource>(id, codec, width, height);
  EncodedSourceRegistry::instance().register_source(id, this);
  RTC_LOG(LS_INFO) << "EncodedVideoTrackSource created id=" << id
                   << " codec=" << static_cast<int>(codec) << " " << width
                   << "x" << height;
}

EncodedVideoTrackSource::~EncodedVideoTrackSource() {
  EncodedSourceRegistry::instance().unregister_source(source_->source_id());
  RTC_LOG(LS_INFO) << "EncodedVideoTrackSource destroyed id="
                   << source_->source_id();
}

bool EncodedVideoTrackSource::capture_frame(rust::Slice<const uint8_t> data,
                                            bool is_keyframe,
                                            bool has_sps_pps,
                                            uint32_t width,
                                            uint32_t height,
                                            int64_t capture_time_us) const {
  std::vector<uint8_t> buf(data.begin(), data.end());
  return source_->push_encoded_frame(std::move(buf), is_keyframe, has_sps_pps,
                                     width, height, capture_time_us);
}

void EncodedVideoTrackSource::set_observer(
    rust::Box<EncodedVideoSourceWrapper> observer) const {
  source_->set_observer(std::move(observer));
}

std::shared_ptr<EncodedVideoTrackSource> new_encoded_video_track_source(
    EncodedVideoCodecType codec,
    uint32_t width,
    uint32_t height) {
  return std::make_shared<EncodedVideoTrackSource>(codec, width, height);
}

}  // namespace livekit_ffi
