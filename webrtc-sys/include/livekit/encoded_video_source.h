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

#pragma once

#include <atomic>
#include <memory>
#include <queue>
#include <unordered_map>

#include "api/video/video_frame.h"
#include "api/video/i420_buffer.h"
#include "livekit/video_track.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class EncodedVideoTrackSource;
class KeyFrameRequestObserverWrapper;
}  // namespace livekit_ffi
#include "webrtc-sys/src/encoded_video_source.rs.h"

namespace livekit_ffi {

/// Holds a single queued encoded frame payload.
struct EncodedFrameData {
  std::vector<uint8_t> data;
  int64_t capture_time_us;
  uint32_t rtp_timestamp;
  uint32_t width;
  uint32_t height;
  bool is_keyframe;
  bool has_sps_pps;
  uint32_t simulcast_index = 0;
};

/// A video track source that accepts pre-encoded frames.
///
/// When `capture_encoded_frame()` is called the encoded payload is queued and
/// a tiny 1×1 dummy raw frame is pushed through the normal
/// `AdaptedVideoTrackSource::OnFrame()` path so that WebRTC's encoding
/// pipeline fires.  The paired `PassthroughVideoEncoder` pulls the queued
/// data out of this source instead of actually encoding.
class EncodedVideoTrackSource {
  class InternalSource : public webrtc::AdaptedVideoTrackSource {
   public:
    InternalSource(const VideoResolution& resolution);
    ~InternalSource() override;

    bool is_screencast() const override;
    std::optional<bool> needs_denoising() const override;
    SourceState state() const override;
    bool remote() const override;
    VideoResolution video_resolution() const;

    /// Enqueue an encoded frame and trigger the encode pipeline.
    bool capture_encoded_frame(rust::Slice<const uint8_t> data,
                               int64_t capture_time_us,
                               uint32_t rtp_timestamp,
                               uint32_t width,
                               uint32_t height,
                               bool is_keyframe,
                               bool has_sps_pps,
                               uint32_t simulcast_index);

    /// Called by PassthroughVideoEncoder::Encode() to retrieve the next
    /// queued encoded payload for a given simulcast layer.
    std::optional<EncodedFrameData> dequeue_frame(uint32_t simulcast_index);

    /// Set by the encoder when WebRTC requests a keyframe.
    void request_keyframe();
    bool consume_keyframe_request();

   private:
    mutable webrtc::Mutex mutex_;
    webrtc::TimestampAligner timestamp_aligner_;
    VideoResolution resolution_;
    std::unordered_map<uint32_t, std::queue<EncodedFrameData>>
        frame_queues_ RTC_GUARDED_BY(mutex_);
    std::atomic<bool> keyframe_requested_{false};
    webrtc::scoped_refptr<webrtc::I420Buffer> dummy_buffer_;
  };

 public:
  EncodedVideoTrackSource(const VideoResolution& resolution,
                          VideoCodecType codec);

  VideoResolution video_resolution() const;
  VideoCodecType codec_type() const;

  void set_keyframe_request_callback(
      rust::Box<KeyFrameRequestObserverWrapper> observer) const;

  webrtc::scoped_refptr<InternalSource> get() const;

 private:
  webrtc::scoped_refptr<InternalSource> source_;
  VideoCodecType codec_;
  mutable webrtc::Mutex cb_mutex_;
  mutable std::unique_ptr<rust::Box<KeyFrameRequestObserverWrapper>>
      keyframe_observer_ RTC_GUARDED_BY(cb_mutex_);

  friend class PassthroughVideoEncoder;
};

std::shared_ptr<EncodedVideoTrackSource> new_encoded_video_track_source(
    uint32_t width,
    uint32_t height,
    VideoCodecType codec);

/// Free function bridge for CXX -- delegates to InternalSource
bool capture_encoded_frame(const EncodedVideoTrackSource& source,
                           rust::Slice<const uint8_t> data,
                           int64_t capture_time_us,
                           uint32_t rtp_timestamp,
                           uint32_t width,
                           uint32_t height,
                           bool is_keyframe,
                           bool has_sps_pps,
                           uint32_t simulcast_index);

static std::shared_ptr<EncodedVideoTrackSource>
_shared_encoded_video_track_source() {
  return nullptr;  // Ignore -- needed for CXX SharedPtr codegen
}

}  // namespace livekit_ffi
