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

#ifndef LIVEKIT_RTC_ENCODED_VIDEO_SOURCE_H
#define LIVEKIT_RTC_ENCODED_VIDEO_SOURCE_H

#include <deque>
#include <functional>
#include <optional>
#include <unordered_map>

#include "api/scoped_refptr.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "api/video_codecs/video_codec.h"
#include "livekit_rtc/include/capi.h"
#include "livekit_rtc/passthrough_encoder.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/ref_count.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"

namespace livekit_ffi {

// Callback for keyframe requests from the encoder
using KeyFrameRequestCallback = std::function<void()>;

// Global registry to connect encoded video sources with their passthrough encoders
// The passthrough encoder uses the frame's ID to find the right provider
class EncodedVideoSourceRegistry {
 public:
  static EncodedVideoSourceRegistry& GetInstance();

  void Register(uint16_t frame_id,
                EncodedFrameProvider* provider,
                webrtc::VideoCodecType codec_type);
  void Unregister(uint16_t frame_id);

  EncodedFrameProvider* GetProvider(uint16_t frame_id);
  webrtc::VideoCodecType GetCodecType(uint16_t frame_id);
  bool IsEncodedSource(uint16_t frame_id);

  // Check if any encoded sources exist for the given codec type
  bool HasSourceForCodec(webrtc::VideoCodecType codec_type);

 private:
  EncodedVideoSourceRegistry() = default;

  struct SourceInfo {
    EncodedFrameProvider* provider;
    webrtc::VideoCodecType codec_type;
  };

  webrtc::Mutex mutex_;
  std::unordered_map<uint16_t, SourceInfo> sources_;
};

// Interface for providing encoded frames to the passthrough encoder
class EncodedFrameProvider {
 public:
  virtual ~EncodedFrameProvider() = default;
  virtual std::optional<PreEncodedFrame> GetNextEncodedFrame() = 0;
  virtual void RequestKeyFrame() = 0;
};

// Video source that accepts pre-encoded frames (H264, VP8, etc.)
// Internally triggers the encoding pipeline with dummy frames while
// the actual encoded data is passed through via EncodedFrameProvider.
class EncodedVideoSource : public webrtc::RefCountInterface,
                           public EncodedFrameProvider {
  class InternalSource : public webrtc::AdaptedVideoTrackSource {
   public:
    InternalSource(uint32_t width, uint32_t height, uint16_t source_id);
    ~InternalSource() override;

    // AdaptedVideoTrackSource interface
    bool is_screencast() const override;
    std::optional<bool> needs_denoising() const override;
    SourceState state() const override;
    bool remote() const override;

    lkVideoResolution video_resolution() const;

    // Push a dummy frame to trigger the encoding pipeline
    void PushDummyFrame(int64_t timestamp_us, uint32_t rtp_timestamp);

    uint16_t source_id() const { return source_id_; }

   private:
    mutable webrtc::Mutex mutex_;
    webrtc::TimestampAligner timestamp_aligner_;
    lkVideoResolution resolution_;
    rtc::scoped_refptr<webrtc::I420Buffer> dummy_buffer_;
    uint16_t source_id_;
  };

 public:
  EncodedVideoSource(uint32_t width,
                     uint32_t height,
                     webrtc::VideoCodecType codec_type);
  ~EncodedVideoSource() override;

  // Capture a pre-encoded frame
  // This queues the encoded data and triggers a dummy frame through the pipeline
  bool CaptureEncodedFrame(const uint8_t* data,
                           uint32_t size,
                           int64_t capture_time_us,
                           uint32_t rtp_timestamp,
                           uint32_t width,
                           uint32_t height,
                           bool is_keyframe,
                           bool has_sps_pps);

  // Set callback for keyframe requests from the encoder
  void SetKeyFrameRequestCallback(KeyFrameRequestCallback callback);

  // EncodedFrameProvider interface
  std::optional<PreEncodedFrame> GetNextEncodedFrame() override;
  void RequestKeyFrame() override;

  // Get the internal video track source
  rtc::scoped_refptr<InternalSource> GetSource() const;

  // Get the codec type for this source
  webrtc::VideoCodecType GetCodecType() const { return codec_type_; }

  // Get the unique source ID
  uint16_t GetSourceId() const { return source_id_; }

  lkVideoResolution video_resolution() const;

 private:
  static uint16_t GetNextSourceId();

  rtc::scoped_refptr<InternalSource> source_;
  webrtc::VideoCodecType codec_type_;
  uint16_t source_id_;

  mutable webrtc::Mutex mutex_;
  std::deque<PreEncodedFrame> pending_frames_;
  KeyFrameRequestCallback keyframe_callback_;
  bool keyframe_requested_ = false;
};

}  // namespace livekit_ffi

#endif  // LIVEKIT_RTC_ENCODED_VIDEO_SOURCE_H
