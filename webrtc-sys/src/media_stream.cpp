/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/media_stream.h"

#include <algorithm>
#include <iostream>
#include <memory>

#include "api/media_stream_interface.h"
#include "api/video/video_frame.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/time_utils.h"

namespace livekit {

MediaStream::MediaStream(
    rtc::scoped_refptr<webrtc::MediaStreamInterface> stream)
    : media_stream_(std::move(stream)) {}

rust::String MediaStream::id() const {
  return media_stream_->id();
}

rust::Vec<VideoTrackPtr> MediaStream::get_video_tracks() const {
  rust::Vec<VideoTrackPtr> rust;
  for (auto video : media_stream_->GetVideoTracks())
    rust.push_back(VideoTrackPtr{std::make_shared<VideoTrack>(video)});

  return rust;
}

rust::Vec<AudioTrackPtr> MediaStream::get_audio_tracks() const {
  rust::Vec<AudioTrackPtr> rust;
  for (auto audio : media_stream_->GetAudioTracks())
    rust.push_back(AudioTrackPtr{std::make_shared<AudioTrack>(audio)});

  return rust;
}

std::shared_ptr<AudioTrack> MediaStream::find_audio_track(
    rust::String track_id) const {
  return std::make_shared<AudioTrack>(
      media_stream_->FindAudioTrack(track_id.c_str()));
}

std::shared_ptr<VideoTrack> MediaStream::find_video_track(
    rust::String track_id) const {
  return std::make_shared<VideoTrack>(
      media_stream_->FindVideoTrack(track_id.c_str()));
}

bool MediaStream::add_track(std::shared_ptr<MediaStreamTrack> track) const {
  if (track->kind() == webrtc::MediaStreamTrackInterface::kVideoKind) {
    return media_stream_->AddTrack(
        rtc::scoped_refptr<webrtc::VideoTrackInterface>(
            static_cast<webrtc::VideoTrackInterface*>(track->get().get())));
  } else {
    return media_stream_->AddTrack(
        rtc::scoped_refptr<webrtc::AudioTrackInterface>(
            static_cast<webrtc::AudioTrackInterface*>(track->get().get())));
  }
}

bool MediaStream::remove_track(std::shared_ptr<MediaStreamTrack> track) const {
  if (track->kind() == webrtc::MediaStreamTrackInterface::kVideoKind) {
    return media_stream_->RemoveTrack(
        rtc::scoped_refptr<webrtc::VideoTrackInterface>(
            static_cast<webrtc::VideoTrackInterface*>(track->get().get())));
  } else {
    return media_stream_->RemoveTrack(
        rtc::scoped_refptr<webrtc::AudioTrackInterface>(
            static_cast<webrtc::AudioTrackInterface*>(track->get().get())));
  }
}

MediaStreamTrack::MediaStreamTrack(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track)
    : track_(std::move(track)) {}

std::shared_ptr<MediaStreamTrack> MediaStreamTrack::from(
    rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track) {
  if (track->kind() == webrtc::MediaStreamTrackInterface::kVideoKind) {
    return std::make_shared<VideoTrack>(
        rtc::scoped_refptr<webrtc::VideoTrackInterface>(
            static_cast<webrtc::VideoTrackInterface*>(track.get())));
  } else {
    return std::make_shared<AudioTrack>(
        rtc::scoped_refptr<webrtc::AudioTrackInterface>(
            static_cast<webrtc::AudioTrackInterface*>(track.get())));
  }
}

rust::String MediaStreamTrack::kind() const {
  return track_->kind();
}

rust::String MediaStreamTrack::id() const {
  return track_->id();
}

bool MediaStreamTrack::enabled() const {
  return track_->enabled();
}

bool MediaStreamTrack::set_enabled(bool enable) const {
  return track_->set_enabled(enable);
}

TrackState MediaStreamTrack::state() const {
  return static_cast<TrackState>(track_->state());
}

AudioTrack::AudioTrack(rtc::scoped_refptr<webrtc::AudioTrackInterface> track)
    : MediaStreamTrack(std::move(track)) {}

VideoTrack::VideoTrack(rtc::scoped_refptr<webrtc::VideoTrackInterface> track)
    : MediaStreamTrack(std::move(track)) {}

void VideoTrack::add_sink(NativeVideoFrameSink& sink) const {
  track()->AddOrUpdateSink(&sink, rtc::VideoSinkWants());
}

void VideoTrack::remove_sink(NativeVideoFrameSink& sink) const {
  track()->RemoveSink(&sink);
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

NativeVideoFrameSink::NativeVideoFrameSink(
    rust::Box<VideoFrameSinkWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeVideoFrameSink::OnFrame(const webrtc::VideoFrame& frame) {
  observer_->on_frame(std::make_unique<VideoFrame>(frame));
}

void NativeVideoFrameSink::OnDiscardedFrame() {
  observer_->on_discarded_frame();
}

void NativeVideoFrameSink::OnConstraintsChanged(
    const webrtc::VideoTrackSourceConstraints& constraints) {
  VideoTrackSourceConstraints cst;
  cst.min_fps = constraints.min_fps.value_or(-1);
  cst.max_fps = constraints.max_fps.value_or(-1);
  observer_->on_constraints_changed(cst);
}

std::unique_ptr<NativeVideoFrameSink> new_native_video_frame_sink(
    rust::Box<VideoFrameSinkWrapper> observer) {
  return std::make_unique<NativeVideoFrameSink>(std::move(observer));
}

NativeVideoTrackSource::NativeVideoTrackSource()
    : rtc::AdaptedVideoTrackSource(4) {}

NativeVideoTrackSource::~NativeVideoTrackSource() {}

bool NativeVideoTrackSource::is_screencast() const {
  return false;
}

absl::optional<bool> NativeVideoTrackSource::needs_denoising() const {
  return false;
}

webrtc::MediaSourceInterface::SourceState NativeVideoTrackSource::state()
    const {
  return SourceState::kLive;
}

bool NativeVideoTrackSource::remote() const {
  return false;
}

bool NativeVideoTrackSource::on_captured_frame(
    const webrtc::VideoFrame& frame) {
  webrtc::MutexLock lock(&mutex_);

  int64_t aligned_timestamp_us = timestamp_aligner_.TranslateTimestamp(
      frame.timestamp_us(), rtc::TimeMicros());

  rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer =
      frame.video_frame_buffer();

  int adapted_width, adapted_height, crop_width, crop_height, crop_x, crop_y;
  if (!AdaptFrame(buffer->width(), buffer->height(), aligned_timestamp_us,
                  &adapted_width, &adapted_height, &crop_width, &crop_height,
                  &crop_x, &crop_y)) {
    return false;
  }

  if (adapted_width != frame.width() || adapted_height != frame.height()) {
    buffer = buffer->CropAndScale(crop_x, crop_y, crop_width, crop_height,
                                  adapted_width, adapted_height);
  }

  if (apply_rotation() && frame.rotation() != webrtc::kVideoRotation_0) {
    // If the buffer is I420, rtc::AdaptedVideoTrackSource will handle the
    // rotation for us.
    buffer = buffer->ToI420();
  }

  OnFrame(webrtc::VideoFrame::Builder()
              .set_video_frame_buffer(frame.video_frame_buffer())
              .set_rotation(frame.rotation())
              .set_timestamp_us(aligned_timestamp_us)
              .build());

  return true;
}

AdaptedVideoTrackSource::AdaptedVideoTrackSource(
    rtc::scoped_refptr<NativeVideoTrackSource> source)
    : source_(source) {}

bool AdaptedVideoTrackSource::on_captured_frame(
    const std::unique_ptr<VideoFrame>& frame) const {
  auto rtc_frame = frame->get();
  rtc_frame.set_timestamp_us(rtc::TimeMicros());
  return source_->on_captured_frame(rtc_frame);
}

rtc::scoped_refptr<NativeVideoTrackSource> AdaptedVideoTrackSource::get()
    const {
  return source_;
}

std::shared_ptr<AdaptedVideoTrackSource> new_adapted_video_track_source() {
  return std::make_shared<AdaptedVideoTrackSource>(
      rtc::make_ref_counted<NativeVideoTrackSource>());
}

}  // namespace livekit
