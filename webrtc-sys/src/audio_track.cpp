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

#include "livekit/audio_track.h"

#include <algorithm>
#include <iostream>
#include <memory>

#include "api/audio_options.h"
#include "api/media_stream_interface.h"
#include "audio/remix_resample.h"
#include "common_audio/include/audio_util.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/time_utils.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/audio_track.rs.h"

namespace livekit {

inline cricket::AudioOptions to_native_audio_options(
    const AudioSourceOptions& options) {
  cricket::AudioOptions rtc_options{};
  rtc_options.echo_cancellation = options.echo_cancellation;
  rtc_options.noise_suppression = options.noise_suppression;
  rtc_options.auto_gain_control = options.auto_gain_control;
  return rtc_options;
}

inline AudioSourceOptions to_rust_audio_options(
    const cricket::AudioOptions& rtc_options) {
  AudioSourceOptions options{};
  options.echo_cancellation = rtc_options.echo_cancellation.value_or(false);
  options.noise_suppression = rtc_options.noise_suppression.value_or(false);
  options.auto_gain_control = rtc_options.auto_gain_control.value_or(false);
  return options;
}

AudioTrack::AudioTrack(std::shared_ptr<RtcRuntime> rtc_runtime,
                       rtc::scoped_refptr<webrtc::AudioTrackInterface> track)
    : MediaStreamTrack(rtc_runtime, std::move(track)) {}

AudioTrack::~AudioTrack() {
  webrtc::MutexLock lock(&mutex_);
  for (auto& sink : sinks_) {
    track()->RemoveSink(sink.get());
  }
}

void AudioTrack::add_sink(const std::shared_ptr<NativeAudioSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->AddSink(sink.get());
  sinks_.push_back(sink);
}

void AudioTrack::remove_sink(
    const std::shared_ptr<NativeAudioSink>& sink) const {
  webrtc::MutexLock lock(&mutex_);
  track()->RemoveSink(sink.get());
  sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
}

NativeAudioSink::NativeAudioSink(rust::Box<AudioSinkWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeAudioSink::OnData(const void* audio_data,
                             int bits_per_sample,
                             int sample_rate,
                             size_t number_of_channels,
                             size_t number_of_frames) {
  RTC_CHECK_EQ(16, bits_per_sample);
  rust::Slice<const int16_t> data(static_cast<const int16_t*>(audio_data),
                                  number_of_channels * number_of_frames);
  observer_->on_data(data, sample_rate, number_of_channels, number_of_frames);
}

std::shared_ptr<NativeAudioSink> new_native_audio_sink(
    rust::Box<AudioSinkWrapper> observer) {
  return std::make_shared<NativeAudioSink>(std::move(observer));
}

AudioTrackSource::InternalSource::InternalSource(
    const cricket::AudioOptions& options) {}

webrtc::MediaSourceInterface::SourceState
AudioTrackSource::InternalSource::state() const {
  return webrtc::MediaSourceInterface::SourceState::kLive;
}

bool AudioTrackSource::InternalSource::remote() const {
  return false;
}

const cricket::AudioOptions AudioTrackSource::InternalSource::options() const {
  webrtc::MutexLock lock(&mutex_);
  return options_;
}

void AudioTrackSource::InternalSource::set_options(
    const cricket::AudioOptions& options) {
  webrtc::MutexLock lock(&mutex_);
  options_ = options;
}

void AudioTrackSource::InternalSource::AddSink(
    webrtc::AudioTrackSinkInterface* sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.push_back(sink);
}

void AudioTrackSource::InternalSource::RemoveSink(
    webrtc::AudioTrackSinkInterface* sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
}

void AudioTrackSource::InternalSource::on_captured_frame(
    rust::Slice<const int16_t> data,
    uint32_t sample_rate,
    uint32_t number_of_channels,
    size_t number_of_frames) {
  webrtc::MutexLock lock(&mutex_);
  for (auto sink : sinks_) {
    sink->OnData(data.data(), 16, sample_rate, number_of_channels,
                 number_of_frames);
  }
}

AudioTrackSource::AudioTrackSource(AudioSourceOptions options) {
  source_ =
      rtc::make_ref_counted<InternalSource>(to_native_audio_options(options));
}

AudioSourceOptions AudioTrackSource::audio_options() const {
  return to_rust_audio_options(source_->options());
}

void AudioTrackSource::set_audio_options(
    const AudioSourceOptions& options) const {
  source_->set_options(to_native_audio_options(options));
}

void AudioTrackSource::on_captured_frame(rust::Slice<const int16_t> audio_data,
                                         uint32_t sample_rate,
                                         uint32_t number_of_channels,
                                         size_t number_of_frames) const {
  source_->on_captured_frame(audio_data, sample_rate, number_of_channels,
                             number_of_frames);
}

rtc::scoped_refptr<AudioTrackSource::InternalSource> AudioTrackSource::get()
    const {
  return source_;
}

std::shared_ptr<AudioTrackSource> new_audio_track_source(
    AudioSourceOptions options) {
  return std::make_shared<AudioTrackSource>(options);
}

}  // namespace livekit
