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

#include "livekit/video_track.h"

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <iostream>
#include <memory>

#include "api/media_stream_interface.h"
#include "api/video/video_frame.h"
#include "api/video/video_rotation.h"
#include "audio/remix_resample.h"
#include "common_audio/include/audio_util.h"
#include "livekit/media_stream.h"
#include "livekit/video_track.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/time_utils.h"
#include "webrtc-sys/src/video_track.rs.h"

namespace {

const char* BufferTypeToString(webrtc::VideoFrameBuffer::Type type) {
  switch (type) {
    case webrtc::VideoFrameBuffer::Type::kNative:
      return "kNative";
    case webrtc::VideoFrameBuffer::Type::kI420:
      return "kI420";
    case webrtc::VideoFrameBuffer::Type::kI420A:
      return "kI420A";
    case webrtc::VideoFrameBuffer::Type::kI444:
      return "kI444";
    case webrtc::VideoFrameBuffer::Type::kI010:
      return "kI010";
    case webrtc::VideoFrameBuffer::Type::kI210:
      return "kI210";
    case webrtc::VideoFrameBuffer::Type::kI410:
      return "kI410";
    case webrtc::VideoFrameBuffer::Type::kNV12:
      return "kNV12";
  }
  return "unknown";
}

bool PublishDebugEnabled() {
  static const bool enabled = std::getenv("LK_PUBLISH_DEBUG") != nullptr;
  return enabled;
}

void MaybeLogPublishSummary(uint64_t received,
                            uint64_t delivered,
                            uint64_t adapt_rejected,
                            uint64_t crop_scale_dropped,
                            uint64_t rotation_dropped) {
  if (!PublishDebugEnabled()) {
    return;
  }
  if (received <= 10 || received % 100 == 0) {
    std::fprintf(stderr,
                 "[VideoTrackSource] summary: received=%lu delivered=%lu "
                 "adapt_rejected=%lu crop_scale_dropped=%lu "
                 "rotation_dropped=%lu\n",
                 received, delivered, adapt_rejected, crop_scale_dropped,
                 rotation_dropped);
    std::fflush(stderr);
  }
}

}  // namespace

namespace livekit_ffi {

VideoTrack::VideoTrack(std::shared_ptr<RtcRuntime> rtc_runtime,
                       webrtc::scoped_refptr<webrtc::VideoTrackInterface> track)
    : MediaStreamTrack(rtc_runtime, std::move(track)) {}

VideoTrack::~VideoTrack() {
  webrtc::MutexLock lock(&mutex_);
  for (auto& sink : sinks_) {
    track()->RemoveSink(sink.get());
  }
}

void VideoTrack::add_sink(const std::shared_ptr<NativeVideoSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->AddOrUpdateSink(sink.get(),
                           webrtc::VideoSinkWants());  // TODO(theomonnom): Expose
                                                    // VideoSinkWants to Rust?
  sinks_.push_back(sink);
}

void VideoTrack::remove_sink(
    const std::shared_ptr<NativeVideoSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->RemoveSink(sink.get());
  sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
}

void VideoTrack::set_should_receive(bool should_receive) const {
  track()->set_should_receive(should_receive);
}

bool VideoTrack::should_receive() const {
  return track()->should_receive();
}

ContentHint VideoTrack::content_hint() const {
  return static_cast<ContentHint>(track()->content_hint());
}

void VideoTrack::set_content_hint(ContentHint hint) const {
  track()->set_content_hint(
      static_cast<webrtc::VideoTrackInterface::ContentHint>(hint));
}

NativeVideoSink::NativeVideoSink(rust::Box<VideoSinkWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeVideoSink::OnFrame(const webrtc::VideoFrame& frame) {
  observer_->on_frame(std::make_unique<VideoFrame>(frame));
}

void NativeVideoSink::OnDiscardedFrame() {
  observer_->on_discarded_frame();
}

void NativeVideoSink::OnConstraintsChanged(
    const webrtc::VideoTrackSourceConstraints& constraints) {
  VideoTrackSourceConstraints cst;
  cst.has_min_fps = constraints.min_fps.has_value();
  cst.min_fps = constraints.min_fps.value_or(0);
  cst.has_max_fps = constraints.max_fps.has_value();
  cst.max_fps = constraints.max_fps.value_or(0);
  observer_->on_constraints_changed(cst);
}

std::shared_ptr<NativeVideoSink> new_native_video_sink(
    rust::Box<VideoSinkWrapper> observer) {
  return std::make_shared<NativeVideoSink>(std::move(observer));
}

VideoTrackSource::InternalSource::InternalSource(
    const VideoResolution& resolution, bool is_screencast)
    : webrtc::AdaptedVideoTrackSource(4), resolution_(resolution), is_screencast_(is_screencast) {}

VideoTrackSource::InternalSource::~InternalSource() {}

bool VideoTrackSource::InternalSource::is_screencast() const {
  return is_screencast_;
}

std::optional<bool> VideoTrackSource::InternalSource::needs_denoising() const {
  return false;
}

webrtc::MediaSourceInterface::SourceState
VideoTrackSource::InternalSource::state() const {
  return SourceState::kLive;
}

bool VideoTrackSource::InternalSource::remote() const {
  return false;
}

VideoResolution VideoTrackSource::InternalSource::video_resolution() const {
  webrtc::MutexLock lock(&mutex_);
  return resolution_;
}

bool VideoTrackSource::InternalSource::on_captured_frame(
    const webrtc::VideoFrame& frame) {
  static std::atomic<uint64_t> received_count(0);
  static std::atomic<uint64_t> delivered_count(0);
  static std::atomic<uint64_t> adapt_rejected_count(0);
  static std::atomic<uint64_t> crop_scale_drop_count(0);
  static std::atomic<uint64_t> rotation_drop_count(0);

  webrtc::MutexLock lock(&mutex_);
  const uint64_t received = received_count.fetch_add(1) + 1;

  int64_t aligned_timestamp_us = timestamp_aligner_.TranslateTimestamp(
      frame.timestamp_us(), webrtc::TimeMicros());

  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer =
      frame.video_frame_buffer();
  const auto buffer_type = buffer->type();
  const bool debug = PublishDebugEnabled();
  if (debug && (received <= 10 || received % 100 == 0)) {
    std::fprintf(stderr,
                 "[VideoTrackSource] on_captured_frame #%lu: frame=%dx%d "
                 "buffer=%s(%d) timestamp_us=%lld aligned_timestamp_us=%lld "
                 "rotation=%d source_resolution=%ux%u\n",
                 received, frame.width(), frame.height(),
                 BufferTypeToString(buffer_type), static_cast<int>(buffer_type),
                 static_cast<long long>(frame.timestamp_us()),
                 static_cast<long long>(aligned_timestamp_us),
                 static_cast<int>(frame.rotation()), resolution_.width,
                 resolution_.height);
    std::fflush(stderr);
  }

  if (resolution_.height == 0 || resolution_.width == 0) {
    resolution_ = VideoResolution{static_cast<uint32_t>(buffer->width()),
                                  static_cast<uint32_t>(buffer->height())};
  }

  int adapted_width, adapted_height, crop_width, crop_height, crop_x, crop_y;
  const bool adapted = AdaptFrame(buffer->width(), buffer->height(),
                                  aligned_timestamp_us, &adapted_width,
                                  &adapted_height, &crop_width, &crop_height,
                                  &crop_x, &crop_y);
  if (debug && (received <= 10 || received % 100 == 0)) {
    std::fprintf(stderr,
                 "[VideoTrackSource] AdaptFrame #%lu: accepted=%d input=%dx%d "
                 "adapted=%dx%d crop=%dx%d@%d,%d\n",
                 received, adapted ? 1 : 0, buffer->width(), buffer->height(),
                 adapted_width, adapted_height, crop_width, crop_height, crop_x,
                 crop_y);
    std::fflush(stderr);
  }
  if (!adapted) {
    const uint64_t adapt_rejected = adapt_rejected_count.fetch_add(1) + 1;
    if (debug) {
      std::fprintf(stderr,
                   "[VideoTrackSource] Dropping frame #%lu because AdaptFrame "
                   "rejected it (buffer=%s, timestamp_us=%lld)\n",
                   received, BufferTypeToString(buffer_type),
                   static_cast<long long>(aligned_timestamp_us));
      std::fflush(stderr);
    }
    MaybeLogPublishSummary(received, delivered_count.load(), adapt_rejected,
                           crop_scale_drop_count.load(),
                           rotation_drop_count.load());
    return false;
  }

  if (adapted_width != frame.width() || adapted_height != frame.height()) {
    if (buffer_type == webrtc::VideoFrameBuffer::Type::kNative) {
      crop_scale_drop_count.fetch_add(1);
      if (debug && (received <= 10 || received % 100 == 0)) {
        std::fprintf(stderr,
                     "[VideoTrackSource] Native frame #%lu: ignoring "
                     "CropAndScale request %dx%d -> %dx%d, delivering at "
                     "original resolution\n",
                     received, frame.width(), frame.height(), adapted_width,
                     adapted_height);
        std::fflush(stderr);
      }
    } else {
      buffer = buffer->CropAndScale(crop_x, crop_y, crop_width, crop_height,
                                    adapted_width, adapted_height);
    }
  }

  webrtc::VideoRotation rotation = frame.rotation();
  if (apply_rotation() && rotation != webrtc::kVideoRotation_0) {
    if (buffer_type == webrtc::VideoFrameBuffer::Type::kNative) {
      const uint64_t rotation_dropped = rotation_drop_count.fetch_add(1) + 1;
      RTC_LOG(LS_WARNING)
          << "Dropping native frame because rotation " << rotation
          << " was requested with apply_rotation(). Native rotation would "
             "fall back to ToI420().";
      if (debug) {
        std::fprintf(stderr,
                     "[VideoTrackSource] Dropping native frame #%lu because "
                     "apply_rotation requested rotation=%d\n",
                     received, static_cast<int>(rotation));
        std::fflush(stderr);
      }
      MaybeLogPublishSummary(received, delivered_count.load(),
                             adapt_rejected_count.load(),
                             crop_scale_drop_count.load(), rotation_dropped);
      return false;
    }
    // If the buffer is I420, webrtc::AdaptedVideoTrackSource will handle the
    // rotation for us.
    buffer = buffer->ToI420();
  }

  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(buffer)
              .set_rotation(rotation)
              .set_timestamp_us(aligned_timestamp_us)
              .build());

  const uint64_t delivered = delivered_count.fetch_add(1) + 1;
  if (debug && (received <= 10 || received % 100 == 0)) {
    std::fprintf(stderr,
                 "[VideoTrackSource] OnFrame delivered #%lu: output=%dx%d "
                 "rotation=%d delivered_total=%lu\n",
                 received, buffer->width(), buffer->height(),
                 static_cast<int>(rotation), delivered);
    std::fflush(stderr);
  }
  MaybeLogPublishSummary(received, delivered, adapt_rejected_count.load(),
                         crop_scale_drop_count.load(),
                         rotation_drop_count.load());

  return true;
}

VideoTrackSource::VideoTrackSource(const VideoResolution& resolution, bool is_screencast) {
  source_ = webrtc::make_ref_counted<InternalSource>(resolution, is_screencast);
}

VideoResolution VideoTrackSource::video_resolution() const {
  return source_->video_resolution();
}

bool VideoTrackSource::on_captured_frame(
    const std::unique_ptr<VideoFrame>& frame) const {
  auto rtc_frame = frame->get();
  return source_->on_captured_frame(rtc_frame);
}

webrtc::scoped_refptr<VideoTrackSource::InternalSource> VideoTrackSource::get()
    const {
  return source_;
}

std::shared_ptr<VideoTrackSource> new_video_track_source(
    const VideoResolution& resolution, bool is_screencast) {
  return std::make_shared<VideoTrackSource>(resolution, is_screencast);
}

}  // namespace livekit_ffi
