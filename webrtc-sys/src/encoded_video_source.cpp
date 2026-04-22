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
#include <cstring>
#include <utility>

#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video/video_rotation.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

namespace {

// ---- Annex-B NAL unit parsing ----
//
// Produces a list of NAL units in the bytestream. Each NalUnit records the
// offset to its leading start code (00 00 01 or 00 00 00 01) and the
// payload offset/length (the bytes after the start code, up to the next
// start code or end of buffer).

struct NalUnit {
  size_t start_code_offset;  // index of the first 0x00 of the start code
  size_t start_code_length;  // 3 or 4
  size_t payload_offset;     // index of the first byte after the start code
  size_t payload_length;     // length of the NAL unit payload (no start code)
  uint8_t first_byte;        // payload[0] — used for NAL type extraction
};

std::vector<NalUnit> ScanNalUnits(const uint8_t* data, size_t size) {
  std::vector<NalUnit> units;
  if (size < 3) return units;

  // Locate start code candidates: positions where data[i..i+2] == 00 00 01.
  // Track them in order; then materialize units with proper payload lengths.
  std::vector<std::pair<size_t, size_t>> starts;  // (offset, length)
  for (size_t i = 0; i + 2 < size;) {
    if (data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1) {
      size_t off = i;
      size_t len = 3;
      if (i > 0 && data[i - 1] == 0) {
        off = i - 1;
        len = 4;
      }
      starts.emplace_back(off, len);
      i += 3;
    } else {
      ++i;
    }
  }

  for (size_t j = 0; j < starts.size(); ++j) {
    NalUnit u;
    u.start_code_offset = starts[j].first;
    u.start_code_length = starts[j].second;
    u.payload_offset = u.start_code_offset + u.start_code_length;
    size_t payload_end =
        (j + 1 < starts.size()) ? starts[j + 1].first : size;
    if (payload_end < u.payload_offset) continue;
    u.payload_length = payload_end - u.payload_offset;
    u.first_byte = u.payload_length > 0 ? data[u.payload_offset] : 0;
    units.push_back(u);
  }
  return units;
}

// H.264 NAL unit types we care about.
enum : uint8_t {
  kH264NalSps = 7,
  kH264NalPps = 8,
};

// H.265 NAL unit types we care about.
enum : uint8_t {
  kH265NalVps = 32,
  kH265NalSps = 33,
  kH265NalPps = 34,
};

uint8_t H264NalType(uint8_t byte) { return byte & 0x1Fu; }
uint8_t H265NalType(uint8_t byte) { return (byte >> 1) & 0x3Fu; }

// Copies [start_code_offset, payload_end) into `out`, including the start
// code. `out` is overwritten.
void CopyNalWithStartCode(const uint8_t* data,
                          const NalUnit& u,
                          std::vector<uint8_t>& out) {
  const size_t total = u.start_code_length + u.payload_length;
  out.assign(data + u.start_code_offset,
             data + u.start_code_offset + total);
}

}  // namespace

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

    // For H.264 / H.265, cache parameter sets we see in the bytestream and
    // auto-prepend them to keyframes that arrive without inline params.
    // Delta frames are passed through unchanged — receivers carry the last
    // seen parameter sets across the stream.
    const bool param_sets_applicable =
        (codec_ == EncodedVideoCodecType::H264 ||
         codec_ == EncodedVideoCodecType::H265);

    if (param_sets_applicable) {
      const auto units = ScanNalUnits(data.data(), data.size());
      bool saw_sps = false;
      bool saw_pps = false;
      bool saw_vps = false;
      for (const auto& u : units) {
        if (codec_ == EncodedVideoCodecType::H264) {
          const uint8_t t = H264NalType(u.first_byte);
          if (t == kH264NalSps) {
            CopyNalWithStartCode(data.data(), u, cached_sps_);
            saw_sps = true;
          } else if (t == kH264NalPps) {
            CopyNalWithStartCode(data.data(), u, cached_pps_);
            saw_pps = true;
          }
        } else {  // H.265
          const uint8_t t = H265NalType(u.first_byte);
          if (t == kH265NalVps) {
            CopyNalWithStartCode(data.data(), u, cached_vps_);
            saw_vps = true;
          } else if (t == kH265NalSps) {
            CopyNalWithStartCode(data.data(), u, cached_sps_);
            saw_sps = true;
          } else if (t == kH265NalPps) {
            CopyNalWithStartCode(data.data(), u, cached_pps_);
            saw_pps = true;
          }
        }
      }

      if (is_keyframe) {
        // Required params for this codec.
        const bool h265 = codec_ == EncodedVideoCodecType::H265;
        const bool have_required =
            !cached_sps_.empty() && !cached_pps_.empty() &&
            (!h265 || !cached_vps_.empty());
        const bool frame_missing =
            !(saw_sps && saw_pps && (!h265 || saw_vps));

        if (frame_missing && have_required) {
          // Prepend cached params. (void)has_sps_pps — we trust the
          // scanner over the flag so callers can't accidentally double-
          // prepend or lie about the contents.
          std::vector<uint8_t> prefixed;
          prefixed.reserve(cached_vps_.size() + cached_sps_.size() +
                           cached_pps_.size() + data.size());
          if (h265) {
            prefixed.insert(prefixed.end(), cached_vps_.begin(),
                            cached_vps_.end());
          }
          prefixed.insert(prefixed.end(), cached_sps_.begin(),
                          cached_sps_.end());
          prefixed.insert(prefixed.end(), cached_pps_.begin(),
                          cached_pps_.end());
          prefixed.insert(prefixed.end(), data.begin(), data.end());
          data = std::move(prefixed);
          has_sps_pps = true;
        } else if (frame_missing) {
          RTC_LOG(LS_WARNING)
              << "EncodedVideoTrackSource[" << source_id_
              << "] keyframe is missing parameter sets and none are cached; "
                 "receiver will fail to decode until the producer emits a "
                 "keyframe with inline SPS/PPS"
              << (h265 ? "/VPS" : "");
        } else {
          // Frame already carries required params (producer inlined them).
          has_sps_pps = true;
        }
      }
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
