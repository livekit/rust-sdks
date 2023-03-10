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

#pragma once

#include <memory>

#include "api/media_stream_interface.h"
#include "api/video/video_frame.h"
#include "livekit/helper.h"
#include "livekit/video_frame.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"
#include "rust/cxx.h"
#include "system_wrappers/include/clock.h"

namespace livekit {
class MediaStream;
class MediaStreamTrack;
class VideoTrack;
class AudioTrack;
class NativeVideoFrameSink;
class AdaptedVideoTrackSource;
}  // namespace livekit
#include "webrtc-sys/src/media_stream.rs.h"

namespace livekit {

class MediaStream {
 public:
  explicit MediaStream(rtc::scoped_refptr<webrtc::MediaStreamInterface> stream);

  rust::String id() const;
  rust::Vec<VideoTrackPtr> get_video_tracks() const;
  rust::Vec<AudioTrackPtr> get_audio_tracks() const;

  std::shared_ptr<AudioTrack> find_audio_track(rust::String track_id) const;
  std::shared_ptr<VideoTrack> find_video_track(rust::String track_id) const;

  bool add_track(std::shared_ptr<MediaStreamTrack> track) const;
  bool remove_track(std::shared_ptr<MediaStreamTrack> track) const;

 private:
  rtc::scoped_refptr<webrtc::MediaStreamInterface> media_stream_;
};

class MediaStreamTrack {
 protected:
  explicit MediaStreamTrack(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

 public:
  static std::shared_ptr<MediaStreamTrack> from(
      rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track);

  rust::String kind() const;
  rust::String id() const;

  bool enabled() const;
  bool set_enabled(bool enable) const;

  TrackState state() const;

  rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> get() const {
    return track_;
  }

 protected:
  rtc::scoped_refptr<webrtc::MediaStreamTrackInterface> track_;
};

class AudioTrack : public MediaStreamTrack {
 public:
  explicit AudioTrack(rtc::scoped_refptr<webrtc::AudioTrackInterface> track);
};

class VideoTrack : public MediaStreamTrack {
 public:
  explicit VideoTrack(rtc::scoped_refptr<webrtc::VideoTrackInterface> track);

  void add_sink(NativeVideoFrameSink& sink) const;
  void remove_sink(NativeVideoFrameSink& sink) const;

  void set_should_receive(bool should_receive) const;
  bool should_receive() const;
  ContentHint content_hint() const;
  void set_content_hint(ContentHint hint) const;

 private:
  webrtc::VideoTrackInterface* track() const {
    return static_cast<webrtc::VideoTrackInterface*>(track_.get());
  }
};

class NativeVideoFrameSink
    : public rtc::VideoSinkInterface<webrtc::VideoFrame> {
 public:
  explicit NativeVideoFrameSink(rust::Box<VideoFrameSinkWrapper> observer);

  void OnFrame(const webrtc::VideoFrame& frame) override;
  void OnDiscardedFrame() override;
  void OnConstraintsChanged(
      const webrtc::VideoTrackSourceConstraints& constraints) override;

 private:
  rust::Box<VideoFrameSinkWrapper> observer_;
};

std::unique_ptr<NativeVideoFrameSink> new_native_video_frame_sink(
    rust::Box<VideoFrameSinkWrapper> observer);

// Native impl of the WebRTC interface
class NativeVideoTrackSource : public rtc::AdaptedVideoTrackSource {
 public:
  NativeVideoTrackSource();
  ~NativeVideoTrackSource() override;

  bool is_screencast() const override;
  absl::optional<bool> needs_denoising() const override;
  webrtc::MediaSourceInterface::SourceState state() const override;
  bool remote() const override;

  bool on_captured_frame(const webrtc::VideoFrame& frame);

 private:
  webrtc::Mutex mutex_;
  rtc::TimestampAligner timestamp_aligner_;
};

class AdaptedVideoTrackSource {
 public:
  AdaptedVideoTrackSource(rtc::scoped_refptr<NativeVideoTrackSource> source);

  bool on_captured_frame(const std::unique_ptr<VideoFrame>& frame)
      const;  // frames pushed from Rust (+interior mutability)

  rtc::scoped_refptr<NativeVideoTrackSource> get() const;

 private:
  webrtc::Clock* clock_ = webrtc::Clock::GetRealTimeClock();
  rtc::scoped_refptr<NativeVideoTrackSource> source_;
};

std::shared_ptr<AdaptedVideoTrackSource> new_adapted_video_track_source();

static std::shared_ptr<MediaStreamTrack> video_to_media(
    std::shared_ptr<VideoTrack> track) {
  return track;
}

static std::shared_ptr<MediaStreamTrack> audio_to_media(
    std::shared_ptr<AudioTrack> track) {
  return track;
}

static std::shared_ptr<VideoTrack> media_to_video(
    std::shared_ptr<MediaStreamTrack> track) {
  return std::static_pointer_cast<VideoTrack>(track);
}

static std::shared_ptr<AudioTrack> media_to_audio(
    std::shared_ptr<MediaStreamTrack> track) {
  return std::static_pointer_cast<AudioTrack>(track);
}

static std::shared_ptr<MediaStreamTrack> _shared_media_stream_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<AudioTrack> _shared_audio_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<VideoTrack> _shared_video_track() {
  return nullptr;  // Ignore
}

static std::shared_ptr<MediaStream> _shared_media_stream() {
  return nullptr;  // Ignore
}

}  // namespace livekit
