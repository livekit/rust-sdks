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

#include "api/video/i420_buffer.h"
#include "livekit/packet_trailer.h"
#include "livekit/passthrough_video_encoder.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

// ---------- InternalSource ----------

EncodedVideoTrackSource::InternalSource::InternalSource(
    const VideoResolution& resolution)
    : webrtc::AdaptedVideoTrackSource(4), resolution_(resolution) {
  // Minimal valid I420 buffer.  We never look at its contents -- it only
  // exists to satisfy WebRTC's frame-flow invariants.
  dummy_buffer_ = webrtc::I420Buffer::Create(2, 2);
  webrtc::I420Buffer::SetBlack(dummy_buffer_.get());
}

EncodedVideoTrackSource::InternalSource::~InternalSource() = default;

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
    const FrameMetadata& frame_metadata,
    uint32_t rtp_timestamp,
    bool is_keyframe,
    bool has_sps_pps) {
  // Always derive capture timestamps from the WebRTC clock so the dummy
  // frame timing matches what the rest of the pipeline expects.
  int64_t now_us = webrtc::TimeMicros();
  int64_t aligned_ts_us;
  uint32_t width;
  uint32_t height;

  {
    webrtc::MutexLock lock(&mutex_);
    aligned_ts_us = timestamp_aligner_.TranslateTimestamp(now_us, now_us);

    if (frame_metadata.has_packet_trailer && packet_trailer_handler_) {
      packet_trailer_handler_->store_frame_metadata(
          aligned_ts_us, frame_metadata.user_timestamp,
          frame_metadata.frame_id);
    }

    width = resolution_.width;
    height = resolution_.height;

    EncodedFrameData frame;
    frame.data.assign(data.data(), data.data() + data.size());
    frame.capture_time_us = aligned_ts_us;
    frame.rtp_timestamp = rtp_timestamp;
    frame.width = width;
    frame.height = height;
    frame.is_keyframe = is_keyframe;
    frame.has_sps_pps = has_sps_pps;
    frame_queue_.push(std::move(frame));
  }

  // Kick the encode pipeline. PassthroughVideoEncoder will pull the real
  // payload out of `frame_queue_` on the encoder thread.
  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(dummy_buffer_)
              .set_rotation(webrtc::kVideoRotation_0)
              .set_timestamp_us(aligned_ts_us)
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

void EncodedVideoTrackSource::InternalSource::set_packet_trailer_handler(
    std::shared_ptr<PacketTrailerHandler> handler) {
  webrtc::MutexLock lock(&mutex_);
  packet_trailer_handler_ = std::move(handler);
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

void EncodedVideoTrackSource::set_packet_trailer_handler(
    std::shared_ptr<PacketTrailerHandler> handler) const {
  source_->set_packet_trailer_handler(std::move(handler));
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
  auto source = std::make_shared<EncodedVideoTrackSource>(res, codec);

  // Register so the encoder factory can resolve a passthrough encoder
  // when WebRTC asks for one matching this codec.
  EncodedSourceRegistry::instance().register_source(source->get().get(),
                                                    source);

  return source;
}

bool capture_encoded_frame(const EncodedVideoTrackSource& source,
                           rust::Slice<const uint8_t> data,
                           const FrameMetadata& frame_metadata,
                           uint32_t rtp_timestamp,
                           bool is_keyframe,
                           bool has_sps_pps) {
  return source.get()->capture_encoded_frame(data, frame_metadata,
                                             rtp_timestamp, is_keyframe,
                                             has_sps_pps);
}

}  // namespace livekit_ffi
