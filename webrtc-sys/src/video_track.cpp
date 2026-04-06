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
#include <chrono>
#include <cstdio>
#include <iostream>
#include <memory>

#include "api/media_stream_interface.h"
#include "api/video/video_frame.h"
#include "api/video/video_rotation.h"
#include "audio/remix_resample.h"
#include "common_audio/include/audio_util.h"
#include "livekit/dmabuf_video_frame_buffer.h"
#include "livekit/media_stream.h"
#include "livekit/video_track.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/time_utils.h"
#include "webrtc-sys/src/video_track.rs.h"

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
  static const bool debug = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  static std::atomic<uint64_t> frame_count(0);
  static std::atomic<uint64_t> crop_scale_count(0);
  static std::atomic<uint64_t> adapt_drop_count(0);
  static std::atomic<double> sum_adapt_us(0);
  static std::atomic<double> sum_crop_us(0);
  static std::atomic<double> sum_broadcast_us(0);
  static std::atomic<double> sum_total_us(0);
  using Clock = std::chrono::steady_clock;

  auto t_start = Clock::now();

  webrtc::MutexLock lock(&mutex_);

  int64_t aligned_timestamp_us = timestamp_aligner_.TranslateTimestamp(
      frame.timestamp_us(), webrtc::TimeMicros());

  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer =
      frame.video_frame_buffer();

  if (resolution_.height == 0 || resolution_.width == 0) {
    resolution_ = VideoResolution{static_cast<uint32_t>(buffer->width()),
                                  static_cast<uint32_t>(buffer->height())};
  }

  int adapted_width, adapted_height, crop_width, crop_height, crop_x, crop_y;
  auto t_adapt_start = Clock::now();
  if (!AdaptFrame(buffer->width(), buffer->height(), aligned_timestamp_us,
                  &adapted_width, &adapted_height, &crop_width, &crop_height,
                  &crop_x, &crop_y)) {
    if (debug) {
      adapt_drop_count.fetch_add(1, std::memory_order_relaxed);
    }
    return false;
  }
  auto t_adapt_end = Clock::now();

  bool did_crop_scale = false;
  if (adapted_width != frame.width() || adapted_height != frame.height()) {
    did_crop_scale = true;
    buffer = buffer->CropAndScale(crop_x, crop_y, crop_width, crop_height,
                                  adapted_width, adapted_height);
  }
  auto t_crop_end = Clock::now();

  webrtc::VideoRotation rotation = frame.rotation();
  if (apply_rotation() && rotation != webrtc::kVideoRotation_0) {
    buffer = buffer->ToI420();
  }

  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(buffer)
              .set_rotation(rotation)
              .set_timestamp_us(aligned_timestamp_us)
              .build());
  auto t_end = Clock::now();

  if (debug) {
    uint64_t n = frame_count.fetch_add(1, std::memory_order_relaxed) + 1;
    if (did_crop_scale) {
      crop_scale_count.fetch_add(1, std::memory_order_relaxed);
    }
    double adapt_us = std::chrono::duration<double, std::micro>(
        t_adapt_end - t_adapt_start).count();
    double crop_us = std::chrono::duration<double, std::micro>(
        t_crop_end - t_adapt_end).count();
    double broadcast_us = std::chrono::duration<double, std::micro>(
        t_end - t_crop_end).count();
    double total_us = std::chrono::duration<double, std::micro>(
        t_end - t_start).count();
    sum_adapt_us.store(sum_adapt_us.load(std::memory_order_relaxed) + adapt_us,
                       std::memory_order_relaxed);
    sum_crop_us.store(sum_crop_us.load(std::memory_order_relaxed) + crop_us,
                      std::memory_order_relaxed);
    sum_broadcast_us.store(
        sum_broadcast_us.load(std::memory_order_relaxed) + broadcast_us,
        std::memory_order_relaxed);
    sum_total_us.store(
        sum_total_us.load(std::memory_order_relaxed) + total_us,
        std::memory_order_relaxed);

    if (n % 60 == 0) {
      double dn = static_cast<double>(n);
      std::fprintf(stderr,
                   "[VideoTrackSource] on_captured_frame stats (%lu frames): "
                   "avg us: adapt=%.0f crop=%.0f broadcast=%.0f total=%.0f | "
                   "crop_scale=%lu adapt_drop=%lu | "
                   "last: %dx%d -> adapted %dx%d (crop %d,%d %dx%d) buf_type=%d\n",
                   n, sum_adapt_us.load() / dn, sum_crop_us.load() / dn,
                   sum_broadcast_us.load() / dn, sum_total_us.load() / dn,
                   crop_scale_count.load(), adapt_drop_count.load(),
                   frame.width(), frame.height(),
                   adapted_width, adapted_height,
                   crop_x, crop_y, crop_width, crop_height,
                   static_cast<int>(buffer->type()));
      std::fflush(stderr);
    }
  }

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

bool VideoTrackSource::capture_dmabuf_frame(int dmabuf_fd,
                                             int width,
                                             int height,
                                             int pixel_format,
                                             int64_t timestamp_us) const {
  auto dmabuf_pixel_format =
      static_cast<livekit::DmaBufPixelFormat>(pixel_format);
  auto buffer = webrtc::make_ref_counted<livekit::DmaBufVideoFrameBuffer>(
      dmabuf_fd, width, height, dmabuf_pixel_format);

  int64_t ts = timestamp_us;
  if (ts == 0) {
    auto now = std::chrono::system_clock::now().time_since_epoch();
    ts = std::chrono::duration_cast<std::chrono::microseconds>(now).count();
  }

  auto frame = webrtc::VideoFrame::Builder()
                   .set_video_frame_buffer(std::move(buffer))
                   .set_rotation(webrtc::kVideoRotation_0)
                   .set_timestamp_us(ts)
                   .build();

  return source_->on_captured_frame(frame);
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
