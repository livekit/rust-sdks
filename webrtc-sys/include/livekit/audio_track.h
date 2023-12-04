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

#include "api/audio_options.h"
#include "livekit/helper.h"
#include "livekit/media_stream_track.h"
#include "livekit/webrtc.h"
#include "pc/local_audio_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit {
class AudioTrack;
class NativeAudioSink;
class AudioTrackSource;
}  // namespace livekit
#include "webrtc-sys/src/audio_track.rs.h"

namespace livekit {

class AudioTrack : public MediaStreamTrack {
 private:
  friend RtcRuntime;
  AudioTrack(std::shared_ptr<RtcRuntime> rtc_runtime,
             rtc::scoped_refptr<webrtc::AudioTrackInterface> track);

 public:
  ~AudioTrack();

  void add_sink(const std::shared_ptr<NativeAudioSink>& sink) const;
  void remove_sink(const std::shared_ptr<NativeAudioSink>& sink) const;

 private:
  webrtc::AudioTrackInterface* track() const {
    return static_cast<webrtc::AudioTrackInterface*>(track_.get());
  }

  mutable webrtc::Mutex mutex_;

  // Same for VideoTrack:
  // Keep a strong reference to the added sinks, so we don't need to
  // manage the lifetime safety on the Rust side
  mutable std::vector<std::shared_ptr<NativeAudioSink>> sinks_;
};

class NativeAudioSink : public webrtc::AudioTrackSinkInterface {
 public:
  explicit NativeAudioSink(rust::Box<AudioSinkWrapper> observer);
  void OnData(const void* audio_data,
              int bits_per_sample,
              int sample_rate,
              size_t number_of_channels,
              size_t number_of_frames) override;

 private:
  rust::Box<AudioSinkWrapper> observer_;
};

std::shared_ptr<NativeAudioSink> new_native_audio_sink(
    rust::Box<AudioSinkWrapper> observer);

class AudioTrackSource {
  class InternalSource : public webrtc::LocalAudioSource {
   public:
    InternalSource(const cricket::AudioOptions& options);

    SourceState state() const override;
    bool remote() const override;

    const cricket::AudioOptions options() const override;

    void AddSink(webrtc::AudioTrackSinkInterface* sink) override;
    void RemoveSink(webrtc::AudioTrackSinkInterface* sink) override;

    void set_options(const cricket::AudioOptions& options);

    // AudioFrame should always contain 10 ms worth of data (see index.md of
    // acm)
    void on_captured_frame(rust::Slice<const int16_t> audio_data,
                           uint32_t sample_rate,
                           uint32_t number_of_channels,
                           size_t number_of_frames);

   private:
    mutable webrtc::Mutex mutex_;
    std::vector<webrtc::AudioTrackSinkInterface*> sinks_;
    cricket::AudioOptions options_{};
  };

 public:
  AudioTrackSource(AudioSourceOptions options);

  AudioSourceOptions audio_options() const;

  void set_audio_options(const AudioSourceOptions& options) const;

  void on_captured_frame(rust::Slice<const int16_t> audio_data,
                         uint32_t sample_rate,
                         uint32_t number_of_channels,
                         size_t number_of_frames) const;

  rtc::scoped_refptr<InternalSource> get() const;

 private:
  rtc::scoped_refptr<InternalSource> source_;
};

std::shared_ptr<AudioTrackSource> new_audio_track_source(
    AudioSourceOptions options);

static std::shared_ptr<MediaStreamTrack> audio_to_media(
    std::shared_ptr<AudioTrack> track) {
  return track;
}

static std::shared_ptr<AudioTrack> media_to_audio(
    std::shared_ptr<MediaStreamTrack> track) {
  return std::static_pointer_cast<AudioTrack>(track);
}

static std::shared_ptr<AudioTrack> _shared_audio_track() {
  return nullptr;  // Ignore
}

}  // namespace livekit
