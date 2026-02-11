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

#include "livekit/encoded_video_source.h"

#include "api/video/i420_buffer.h"
#include "livekit/passthrough_video_encoder.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

// ---------- InternalSource ----------

EncodedVideoTrackSource::InternalSource::InternalSource(
    const VideoResolution& resolution)
    : webrtc::AdaptedVideoTrackSource(4), resolution_(resolution) {
  // Create a 2x2 dummy I420 buffer (minimum valid size for WebRTC)
  dummy_buffer_ = webrtc::I420Buffer::Create(2, 2);
  // Fill with black
  webrtc::I420Buffer::SetBlack(dummy_buffer_.get());
}

EncodedVideoTrackSource::InternalSource::~InternalSource() {}

bool EncodedVideoTrackSource::InternalSource::is_screencast() const {
  return false;
}

std::optional<bool>
EncodedVideoTrackSource::InternalSource::needs_denoising() const {
  return false;
}

webrtc::MediaSourceInterface::SourceState
EncodedVideoTrackSource::InternalSource::state() const {
  return SourceState::kLive;
}

bool EncodedVideoTrackSource::InternalSource::remote() const {
  return false;
}

VideoResolution
EncodedVideoTrackSource::InternalSource::video_resolution() const {
  webrtc::MutexLock lock(&mutex_);
  return resolution_;
}

bool EncodedVideoTrackSource::InternalSource::capture_encoded_frame(
    rust::Slice<const uint8_t> data,
    int64_t capture_time_us,
    uint32_t rtp_timestamp,
    uint32_t width,
    uint32_t height,
    bool is_keyframe,
    bool has_sps_pps) {
  // Enqueue the encoded data
  {
    webrtc::MutexLock lock(&mutex_);
    EncodedFrameData frame;
    frame.data.assign(data.data(), data.data() + data.size());
    frame.capture_time_us = capture_time_us;
    frame.rtp_timestamp = rtp_timestamp;
    frame.width = width;
    frame.height = height;
    frame.is_keyframe = is_keyframe;
    frame.has_sps_pps = has_sps_pps;
    frame_queue_.push(std::move(frame));
  }

  // Push a dummy frame to trigger the WebRTC encode pipeline.
  // The PassthroughVideoEncoder will pull the real data from our queue.
  int64_t ts_us = capture_time_us;
  if (ts_us == 0) {
    ts_us = webrtc::TimeMicros();
  }

  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(dummy_buffer_)
              .set_rotation(webrtc::kVideoRotation_0)
              .set_timestamp_us(ts_us)
              .build());

  return true;
}

std::optional<EncodedFrameData>
EncodedVideoTrackSource::InternalSource::dequeue_frame() {
  webrtc::MutexLock lock(&mutex_);
  if (frame_queue_.empty()) {
    return std::nullopt;
  }
  EncodedFrameData frame = std::move(frame_queue_.front());
  frame_queue_.pop();
  return frame;
}

void EncodedVideoTrackSource::InternalSource::request_keyframe() {
  keyframe_requested_.store(true, std::memory_order_release);
}

bool EncodedVideoTrackSource::InternalSource::consume_keyframe_request() {
  return keyframe_requested_.exchange(false, std::memory_order_acq_rel);
}

// ---------- EncodedVideoTrackSource ----------

EncodedVideoTrackSource::EncodedVideoTrackSource(
    const VideoResolution& resolution,
    VideoCodecType codec)
    : codec_(codec) {
  source_ = webrtc::make_ref_counted<InternalSource>(resolution);
}

VideoResolution EncodedVideoTrackSource::video_resolution() const {
  return source_->video_resolution();
}

VideoCodecType EncodedVideoTrackSource::codec_type() const {
  return codec_;
}

void EncodedVideoTrackSource::set_keyframe_request_callback(
    rust::Box<KeyFrameRequestObserverWrapper> observer) const {
  webrtc::MutexLock lock(&cb_mutex_);
  keyframe_observer_ =
      std::make_unique<rust::Box<KeyFrameRequestObserverWrapper>>(
          std::move(observer));
}

webrtc::scoped_refptr<EncodedVideoTrackSource::InternalSource>
EncodedVideoTrackSource::get() const {
  return source_;
}

std::shared_ptr<EncodedVideoTrackSource> new_encoded_video_track_source(
    uint32_t width,
    uint32_t height,
    VideoCodecType codec) {
  VideoResolution res{width, height};
  auto source =
      std::make_shared<EncodedVideoTrackSource>(res, codec);

  // Register in the global registry so the encoder factory can find it
  EncodedSourceRegistry::instance().register_source(
      source->get().get(), source);

  return source;
}

bool capture_encoded_frame(const EncodedVideoTrackSource& source,
                           rust::Slice<const uint8_t> data,
                           int64_t capture_time_us,
                           uint32_t rtp_timestamp,
                           uint32_t width,
                           uint32_t height,
                           bool is_keyframe,
                           bool has_sps_pps) {
  return source.get()->capture_encoded_frame(
      data, capture_time_us, rtp_timestamp, width, height, is_keyframe,
      has_sps_pps);
}

}  // namespace livekit_ffi
