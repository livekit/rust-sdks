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

#include "api/scoped_refptr.h"
#include "livekit/audio_device.h"
#include "livekit/webrtc.h"
#include "modules/audio_mixer/audio_mixer_impl.h"

namespace webrtc {
class TaskQueueFactory;
class AudioDeviceBuffer;
class AudioTransport;
}  // namespace webrtc

namespace livekit {

class AudioContext {
 public:
  AudioContext(std::shared_ptr<RtcRuntime> rtc_runtime);
  virtual ~AudioContext();

  rtc::scoped_refptr<AudioDevice> audio_device(
      webrtc::TaskQueueFactory* task_queue_factory);

  rtc::scoped_refptr<webrtc::AudioMixer> audio_mixer();

  webrtc::AudioDeviceBuffer* audio_device_buffer();

  webrtc::AudioTransport* audio_transport();

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  rtc::scoped_refptr<livekit::AudioDevice> audio_device_;
  rtc::scoped_refptr<webrtc::AudioMixer> audio_mixer_;
};

}  // namespace livekit