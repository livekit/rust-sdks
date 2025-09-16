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

#include "livekit/audio_context.h"

#include "modules/audio_mixer/audio_mixer_impl.h"
#include "rtc_base/thread.h"

namespace livekit {

AudioContext::AudioContext(std::shared_ptr<RtcRuntime> rtc_runtime)
    : rtc_runtime_(rtc_runtime) {}

AudioContext::~AudioContext() {
  if (audio_device_) {
    rtc_runtime_->worker_thread()->BlockingCall(
        [&] { audio_device_ = nullptr; });
  }
  if (audio_mixer_) {
    rtc_runtime_->worker_thread()->BlockingCall(
        [&] { audio_mixer_ = nullptr; });
  }
}

rtc::scoped_refptr<AudioDevice> AudioContext::audio_device(
    webrtc::TaskQueueFactory* task_queue_factory) {
  if (!audio_device_) {
    audio_device_ = rtc_runtime_->worker_thread()->BlockingCall([&] {
      return rtc::make_ref_counted<livekit::AudioDevice>(task_queue_factory);
    });
  }

  return audio_device_;
}

rtc::scoped_refptr<webrtc::AudioMixer> AudioContext::audio_mixer() {
  if (!audio_mixer_) {
    audio_mixer_ = rtc_runtime_->worker_thread()->BlockingCall(
        [&] { return webrtc::AudioMixerImpl::Create(); });
  }
  return audio_mixer_;
}

webrtc::AudioDeviceBuffer* AudioContext::audio_device_buffer() {
  if (audio_device_) {
    return audio_device_->audio_device_buffer();
  }
  return nullptr;
}

webrtc::AudioTransport* AudioContext::audio_transport() {
  if (audio_device_) {
    return audio_device_->audio_transport();
  }
  return nullptr;
}

}  // namespace livekit