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
#include <mutex>
#include <unordered_map>
#include <vector>

#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "api/video/video_frame.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class EncodedVideoTrackSource;
class EncodedVideoSourceWrapper;

}  // namespace livekit_ffi

#include "webrtc-sys/src/encoded_video_source.rs.h"

namespace livekit_ffi {

// Process-global registry that maps a 16-bit source id (stamped on every
// dummy VideoFrame via VideoFrame::set_id) to the owning encoded source.
//
// This is the mechanism the LazyVideoEncoder uses to decide whether to
// instantiate a PassthroughVideoEncoder or a real encoder on the first
// Encode() call. Keying on VideoFrame::id() (rather than codec name) ensures
// per-track routing is correct even when multiple encoded sources share a
// codec.
class EncodedSourceRegistry {
 public:
  static EncodedSourceRegistry& instance();

  // Returns a new non-zero u16 id, skipping any id currently in use.
  uint16_t allocate_id();

  void register_source(uint16_t id, EncodedVideoTrackSource* src);
  void unregister_source(uint16_t id);
  EncodedVideoTrackSource* lookup(uint16_t id);

 private:
  EncodedSourceRegistry() = default;

  std::mutex mu_;
  std::unordered_map<uint16_t, EncodedVideoTrackSource*> map_;
  uint32_t next_id_ = 1;
};

// Owns a single encoded video feed. The paired PassthroughVideoEncoder pops
// frames from this source via the registry (looked up by VideoFrame::id()).
class EncodedVideoTrackSource {
 public:
  class InternalSource : public webrtc::AdaptedVideoTrackSource {
   public:
    InternalSource(uint16_t source_id,
                   EncodedVideoCodecType codec,
                   uint32_t width,
                   uint32_t height);
    ~InternalSource() override;

    bool is_screencast() const override { return false; }
    std::optional<bool> needs_denoising() const override { return std::nullopt; }
    SourceState state() const override { return kLive; }
    bool remote() const override { return false; }

    uint16_t source_id() const { return source_id_; }
    EncodedVideoCodecType codec() const { return codec_; }

    // Enqueues the encoded bytes and pushes one dummy VideoFrame into the
    // WebRTC pipeline so the encoder tick fires. Returns false if the frame
    // was dropped because the queue was full and the frame was not a keyframe.
    bool push_encoded_frame(std::vector<uint8_t> data,
                            bool is_keyframe,
                            bool has_sps_pps,
                            uint32_t width,
                            uint32_t height,
                            int64_t capture_time_us);

    struct DequeuedFrame {
      std::vector<uint8_t> data;
      bool is_keyframe = false;
      bool has_sps_pps = false;
      uint32_t width = 0;
      uint32_t height = 0;
      int64_t capture_time_us = 0;
    };
    bool pop_encoded_frame(DequeuedFrame& out);

    // Wired into PassthroughVideoEncoder::Encode / SetRates so the Rust
    // producer can react to PLI/FIR and congestion control.
    void notify_keyframe_requested();
    void notify_target_bitrate(uint32_t bitrate_bps, double framerate_fps);

    void set_observer(rust::Box<EncodedVideoSourceWrapper> observer);

   private:
    const uint16_t source_id_;
    const EncodedVideoCodecType codec_;

    mutable webrtc::Mutex mutex_;
    std::deque<DequeuedFrame> queue_;
    uint32_t width_;
    uint32_t height_;
    std::unique_ptr<rust::Box<EncodedVideoSourceWrapper>> observer_;

    // Cached H.264/H.265 parameter sets, each with a leading 4-byte Annex-B
    // start code. Populated by scanning incoming keyframes. Prepended to
    // later keyframes that arrive without inline parameter sets.
    //
    // For H.264: vps is unused. For H.265: all three are typically present.
    std::vector<uint8_t> cached_vps_;
    std::vector<uint8_t> cached_sps_;
    std::vector<uint8_t> cached_pps_;

    static constexpr size_t kMaxQueueSize = 8;
  };

  EncodedVideoTrackSource(EncodedVideoCodecType codec,
                          uint32_t width,
                          uint32_t height);
  ~EncodedVideoTrackSource();

  uint16_t source_id() const { return source_->source_id(); }
  EncodedVideoCodecType codec() const { return source_->codec(); }

  bool capture_frame(rust::Slice<const uint8_t> data,
                     bool is_keyframe,
                     bool has_sps_pps,
                     uint32_t width,
                     uint32_t height,
                     int64_t capture_time_us) const;

  void set_observer(rust::Box<EncodedVideoSourceWrapper> observer) const;

  webrtc::scoped_refptr<InternalSource> get() const { return source_; }

 private:
  webrtc::scoped_refptr<InternalSource> source_;
};

std::shared_ptr<EncodedVideoTrackSource> new_encoded_video_track_source(
    EncodedVideoCodecType codec,
    uint32_t width,
    uint32_t height);

}  // namespace livekit_ffi
