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

#include "livekit/apple_audio_engine.h"

#include "modules/audio_device/audio_engine_device.h"

namespace livekit_ffi {

namespace {

// The ADM created with kAppleAudioEngine is always a concrete
// AudioEngineDevice. CreateAudioDeviceModule forwards to
// CreateAudioEngineDeviceModule which constructs the type directly,
// so this downcast is safe.
webrtc::AudioEngineDevice* AsAudioEngineDevice(webrtc::AudioDeviceModule* adm) {
  return static_cast<webrtc::AudioEngineDevice*>(adm);
}

}  // namespace

int32_t AudioEngineSetMuteMode(webrtc::AudioDeviceModule* adm, int32_t mode) {
  if (!adm) {
    return -1;
  }
  if (mode < webrtc::AudioEngineDevice::MuteMode::VoiceProcessing ||
      mode > webrtc::AudioEngineDevice::MuteMode::InputMixer) {
    return -1;
  }
  return AsAudioEngineDevice(adm)->SetMuteMode(
      static_cast<webrtc::AudioEngineDevice::MuteMode>(mode));
}

int32_t AudioEngineGetMuteMode(webrtc::AudioDeviceModule* adm,
                               int32_t* out_mode) {
  if (!adm || !out_mode) {
    return -1;
  }
  webrtc::AudioEngineDevice::MuteMode mode;
  if (AsAudioEngineDevice(adm)->GetMuteMode(&mode) != 0) {
    return -1;
  }
  *out_mode = static_cast<int32_t>(mode);
  return 0;
}

}  // namespace livekit_ffi
