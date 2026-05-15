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
#include <memory>
#include <queue>

#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "livekit/video_track.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"
#include "rust/cxx.h"

namespace livekit_ffi {

class EncodedVideoTrackSource;
class KeyFrameRequestObserverWrapper;
class PacketTrailerHandler;  // forward decl
}  // namespace livekit_ffi
#include "webrtc-sys/src/encoded_video_source.rs.h"

namespace livekit_ffi {

/// One queued payload to be drained by the paired PassthroughVideoEncoder.
///
/// The full timestamp/frame_id tracking is intentionally captured here so the
/// encoder can either propagate the values to the egress packet trailer or
/// emit them as part of the encoded image metadata.
struct EncodedFrameData {
  std::vector<uint8_t> data;
  /// Aligned capture timestamp (us) -- matches the dummy `OnFrame()` timestamp
  /// so the existing `PacketTrailerTransformer` lookup keys line up.
  int64_t capture_time_us;
  uint32_t rtp_timestamp;
  uint32_t width;
  uint32_t height;
  bool is_keyframe;
  bool has_sps_pps;
};

/// A video track source that accepts pre-encoded frames.
///
/// `capture_encoded_frame()` pushes a dummy 2x2 I420 frame through the normal
/// `AdaptedVideoTrackSource::OnFrame()` path which kicks the WebRTC encode
/// pipeline.  The paired `PassthroughVideoEncoder` then dequeues the real
/// encoded payload and forwards it to the RTP sender via
/// `EncodedImageCallback::OnEncodedImage()`.
class EncodedVideoTrackSource {
  class InternalSource : public webrtc::AdaptedVideoTrackSource {
   public:
    explicit InternalSource(const VideoResolution& resolution);
    ~InternalSource() override;

    bool is_screencast() const override;
    std::optional<bool> needs_denoising() const override;
    SourceState state() const override;
    bool remote() const override;
    VideoResolution video_resolution() const;

    /// Enqueue an encoded frame and trigger the encode pipeline.
    bool capture_encoded_frame(rust::Slice<const uint8_t> data,
                               const FrameMetadata& frame_metadata,
                               uint32_t rtp_timestamp,
                               bool is_keyframe,
                               bool has_sps_pps);

    /// Called by `PassthroughVideoEncoder::Encode()` to retrieve the next
    /// queued payload.
    std::optional<EncodedFrameData> dequeue_frame();

    /// Mark a keyframe as requested by the receive side (PLI/FIR).
    void request_keyframe();
    bool consume_keyframe_request();

    void set_packet_trailer_handler(
        std::shared_ptr<PacketTrailerHandler> handler);

   private:
    mutable webrtc::Mutex mutex_;
    webrtc::TimestampAligner timestamp_aligner_;
    VideoResolution resolution_;
    std::queue<EncodedFrameData> frame_queue_ RTC_GUARDED_BY(mutex_);
    std::atomic<bool> keyframe_requested_{false};
    webrtc::scoped_refptr<webrtc::I420Buffer> dummy_buffer_;
    std::shared_ptr<PacketTrailerHandler> packet_trailer_handler_
        RTC_GUARDED_BY(mutex_);
  };

 public:
  EncodedVideoTrackSource(const VideoResolution& resolution,
                          VideoCodecType codec);

  VideoResolution video_resolution() const;
  VideoCodecType codec_type() const;

  void set_keyframe_request_callback(
      rust::Box<KeyFrameRequestObserverWrapper> observer) const;

  void set_packet_trailer_handler(
      std::shared_ptr<PacketTrailerHandler> handler) const;

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

/// Free function bridge for cxx - delegates to the InternalSource.
bool capture_encoded_frame(const EncodedVideoTrackSource& source,
                           rust::Slice<const uint8_t> data,
                           const FrameMetadata& frame_metadata,
                           uint32_t rtp_timestamp,
                           bool is_keyframe,
                           bool has_sps_pps);

static std::shared_ptr<EncodedVideoTrackSource>
_shared_encoded_video_track_source() {
  return nullptr;  // Ignore -- only present for cxx SharedPtr codegen.
}

}  // namespace livekit_ffi
