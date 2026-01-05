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

#include "livekit_rtc/audio_mixer.h"

#include <iostream>
#include <memory>

#include "api/audio/audio_frame.h"
#include "api/audio/audio_mixer.h"
#include "modules/audio_mixer/audio_mixer_impl.h"

namespace livekit {

AudioMixer::AudioMixer() {
  audio_mixer_ = webrtc::AudioMixerImpl::Create();
}

void AudioMixer::add_source(lkAudioMixerSourceCallback* source, void* userdata) {
  auto native_source = std::make_shared<AudioMixerSource>(source, userdata);

  webrtc::MutexLock lock(&sources_mutex_);
  audio_mixer_->AddSource(native_source.get());
  sources_.push_back(native_source);
}

void AudioMixer::remove_source(int source_ssrc) {
  webrtc::MutexLock lock(&sources_mutex_);
  auto it = std::find_if(
      sources_.begin(), sources_.end(),
      [source_ssrc](const auto& s) { return s->Ssrc() == source_ssrc; });

  if (it != sources_.end()) {
    audio_mixer_->RemoveSource(it->get());
    sources_.erase(it);
  }
}

size_t AudioMixer::mix(size_t number_of_channels) {
  audio_mixer_->Mix(number_of_channels, &frame_);
  return frame_.num_channels() * frame_.samples_per_channel();
}

const int16_t* AudioMixer::data() const {
  return frame_.data();
}

std::unique_ptr<AudioMixer> create_audio_mixer() {
  return std::make_unique<AudioMixer>();
}

AudioMixerSource::AudioMixerSource(lkAudioMixerSourceCallback* source,
                                   void* userdata)
    : source_(source), userdata_(userdata) {}

int AudioMixerSource::Ssrc() const {
  return source_->getSsrc(userdata_);
}

int AudioMixerSource::PreferredSampleRate() const {
  return source_->preferredSampleRate(userdata_);
}

webrtc::AudioMixer::Source::AudioFrameInfo
AudioMixerSource::GetAudioFrameWithInfo(int sample_rate,
                                        webrtc::AudioFrame* audio_frame) {
  NativeAudioFrame frame(audio_frame);

  livekit::lkAudioFrameInfo result = source_->getAudioFrameWithInfo(
      sample_rate, static_cast<lkNativeAudioFrame*>(&frame), userdata_);

  if (result == livekit::lkAudioFrameInfo::Normal) {
    return webrtc::AudioMixer::Source::AudioFrameInfo::kNormal;
  } else if (result == livekit::lkAudioFrameInfo::Muted) {
    return webrtc::AudioMixer::Source::AudioFrameInfo::kMuted;
  } else {
    return webrtc::AudioMixer::Source::AudioFrameInfo::kError;
  }
}

void NativeAudioFrame::update_frame(uint32_t timestamp,
                                    const int16_t* data,
                                    size_t samples_per_channel,
                                    int sample_rate_hz,
                                    size_t num_channels) {
  frame_->UpdateFrame(timestamp, data, samples_per_channel, sample_rate_hz,
                      webrtc::AudioFrame::SpeechType::kNormalSpeech,
                      webrtc::AudioFrame::VADActivity::kVadUnknown,
                      num_channels);
}

}  // namespace livekit
