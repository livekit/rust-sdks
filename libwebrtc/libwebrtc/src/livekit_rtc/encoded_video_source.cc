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

#include "livekit_rtc/encoded_video_source.h"

#include <atomic>

#include "api/video/i420_buffer.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

namespace {
std::atomic<uint16_t> g_next_source_id{1};
}

// EncodedVideoSourceRegistry implementation

EncodedVideoSourceRegistry& EncodedVideoSourceRegistry::GetInstance() {
  static EncodedVideoSourceRegistry instance;
  return instance;
}

void EncodedVideoSourceRegistry::Register(uint16_t frame_id,
                                          EncodedFrameProvider* provider,
                                          webrtc::VideoCodecType codec_type) {
  webrtc::MutexLock lock(&mutex_);
  sources_[frame_id] = {provider, codec_type};
}

void EncodedVideoSourceRegistry::Unregister(uint16_t frame_id) {
  webrtc::MutexLock lock(&mutex_);
  sources_.erase(frame_id);
}

EncodedFrameProvider* EncodedVideoSourceRegistry::GetProvider(uint16_t frame_id) {
  webrtc::MutexLock lock(&mutex_);
  auto it = sources_.find(frame_id);
  if (it != sources_.end()) {
    return it->second.provider;
  }
  return nullptr;
}

webrtc::VideoCodecType EncodedVideoSourceRegistry::GetCodecType(uint16_t frame_id) {
  webrtc::MutexLock lock(&mutex_);
  auto it = sources_.find(frame_id);
  if (it != sources_.end()) {
    return it->second.codec_type;
  }
  return webrtc::kVideoCodecGeneric;
}

bool EncodedVideoSourceRegistry::IsEncodedSource(uint16_t frame_id) {
  webrtc::MutexLock lock(&mutex_);
  return sources_.find(frame_id) != sources_.end();
}

bool EncodedVideoSourceRegistry::HasSourceForCodec(
    webrtc::VideoCodecType codec_type) {
  webrtc::MutexLock lock(&mutex_);
  for (const auto& [id, info] : sources_) {
    if (info.codec_type == codec_type) {
      return true;
    }
  }
  return false;
}

// Static helper to get next source ID
uint16_t EncodedVideoSource::GetNextSourceId() {
  return g_next_source_id.fetch_add(1, std::memory_order_relaxed);
}

// InternalSource implementation

EncodedVideoSource::InternalSource::InternalSource(uint32_t width,
                                                   uint32_t height,
                                                   uint16_t source_id)
    : webrtc::AdaptedVideoTrackSource(4), source_id_(source_id) {
  resolution_.width = width;
  resolution_.height = height;

  // Create a small dummy buffer for triggering the encoding pipeline
  // The actual frame data comes from the pre-encoded frames
  dummy_buffer_ = webrtc::I420Buffer::Create(width, height);
  // Initialize with black frame
  webrtc::I420Buffer::SetBlack(dummy_buffer_.get());
}

EncodedVideoSource::InternalSource::~InternalSource() {}

bool EncodedVideoSource::InternalSource::is_screencast() const {
  return false;
}

std::optional<bool> EncodedVideoSource::InternalSource::needs_denoising()
    const {
  return false;
}

webrtc::MediaSourceInterface::SourceState
EncodedVideoSource::InternalSource::state() const {
  return SourceState::kLive;
}

bool EncodedVideoSource::InternalSource::remote() const {
  return false;
}

lkVideoResolution EncodedVideoSource::InternalSource::video_resolution() const {
  webrtc::MutexLock lock(&mutex_);
  return resolution_;
}

void EncodedVideoSource::InternalSource::PushDummyFrame(int64_t timestamp_us,
                                                         uint32_t rtp_timestamp) {
  webrtc::MutexLock lock(&mutex_);

  // Use the provided timestamp directly for consistency with the encoded frame
  // This ensures the dummy frame timing matches what we'll use in the EncodedImage
  auto frame = webrtc::VideoFrame::Builder()
                   .set_video_frame_buffer(dummy_buffer_)
                   .set_rotation(webrtc::kVideoRotation_0)
                   .set_timestamp_us(timestamp_us)
                   .set_timestamp_rtp(rtp_timestamp)
                   .set_id(source_id_)
                   .build();
  OnFrame(frame);
}

// EncodedVideoSource implementation

EncodedVideoSource::EncodedVideoSource(uint32_t width,
                                       uint32_t height,
                                       webrtc::VideoCodecType codec_type)
    : codec_type_(codec_type), source_id_(GetNextSourceId()) {
  source_ = rtc::make_ref_counted<InternalSource>(width, height, source_id_);

  // Register this source so the passthrough encoder can find it
  EncodedVideoSourceRegistry::GetInstance().Register(source_id_, this,
                                                     codec_type_);
}

EncodedVideoSource::~EncodedVideoSource() {
  // Unregister before cleanup
  EncodedVideoSourceRegistry::GetInstance().Unregister(source_id_);

  webrtc::MutexLock lock(&mutex_);
  pending_frames_.clear();
}

bool EncodedVideoSource::CaptureEncodedFrame(const uint8_t* data,
                                             uint32_t size,
                                             int64_t capture_time_us,
                                             uint32_t rtp_timestamp,
                                             uint32_t width,
                                             uint32_t height,
                                             bool is_keyframe,
                                             bool has_sps_pps) {
  if (!data || size == 0) {
    RTC_LOG(LS_WARNING)
        << "EncodedVideoSource::CaptureEncodedFrame: Invalid data";
    return false;
  }

  // Create a copy of the encoded data
  auto buffer = webrtc::EncodedImageBuffer::Create(data, size);

  PreEncodedFrame frame;
  frame.data = buffer;
  frame.capture_time_us = capture_time_us;
  frame.rtp_timestamp = rtp_timestamp;
  frame.width = width;
  frame.height = height;
  frame.is_keyframe = is_keyframe;
  frame.has_sps_pps = has_sps_pps;

  {
    webrtc::MutexLock lock(&mutex_);

    // Limit queue size to prevent unbounded growth
    const size_t kMaxPendingFrames = 30;
    if (pending_frames_.size() >= kMaxPendingFrames) {
      RTC_LOG(LS_WARNING) << "EncodedVideoSource: Dropping frame, queue full";
      pending_frames_.pop_front();
    }

    pending_frames_.push_back(std::move(frame));
  }

  // Trigger the encoding pipeline with a dummy frame
  // The passthrough encoder will retrieve the queued encoded data
  source_->PushDummyFrame(capture_time_us, rtp_timestamp);

  return true;
}

void EncodedVideoSource::SetKeyFrameRequestCallback(
    KeyFrameRequestCallback callback) {
  webrtc::MutexLock lock(&mutex_);
  keyframe_callback_ = std::move(callback);
}

std::optional<PreEncodedFrame> EncodedVideoSource::GetNextEncodedFrame() {
  webrtc::MutexLock lock(&mutex_);

  if (pending_frames_.empty()) {
    return std::nullopt;
  }

  PreEncodedFrame frame = std::move(pending_frames_.front());
  pending_frames_.pop_front();
  return frame;
}

void EncodedVideoSource::RequestKeyFrame() {
  KeyFrameRequestCallback callback;
  {
    webrtc::MutexLock lock(&mutex_);
    keyframe_requested_ = true;
    callback = keyframe_callback_;
  }

  if (callback) {
    callback();
  }
}

rtc::scoped_refptr<EncodedVideoSource::InternalSource>
EncodedVideoSource::GetSource() const {
  return source_;
}

lkVideoResolution EncodedVideoSource::video_resolution() const {
  return source_->video_resolution();
}

}  // namespace livekit_ffi
