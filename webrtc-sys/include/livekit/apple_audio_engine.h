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

#pragma once

#include <cstdint>

namespace webrtc {
class AudioDeviceModule;
}  // namespace webrtc

namespace livekit_ffi {

// Helpers to reach AudioEngineDevice functionality that is not part of the
// base AudioDeviceModule interface. Implemented in apple_audio_engine.mm
// because audio_engine_device.h imports Objective-C frameworks and can only
// be included from an Objective-C++ translation unit.
//
// `adm` must be the ADM created with kAppleAudioEngine, which is guaranteed
// to be a concrete webrtc::AudioEngineDevice (see CreateAudioDeviceModule /
// CreateAudioEngineDeviceModule). Calls must happen on the thread that
// created the ADM (the worker thread).

// Sets the mute mode of the AudioEngine ADM. `mode` uses
// webrtc::AudioEngineDevice::MuteMode values:
// 0 = VoiceProcessing (VPIO mute, engine keeps running, default)
// 1 = RestartEngine (input node torn down, mic indicator turns off)
// 2 = InputMixer (input mixer volume set to 0)
// Returns 0 on success, -1 on invalid arguments or failure.
int32_t AudioEngineSetMuteMode(webrtc::AudioDeviceModule* adm, int32_t mode);

// Reads the current mute mode into `out_mode`.
// Returns 0 on success, -1 on failure.
int32_t AudioEngineGetMuteMode(webrtc::AudioDeviceModule* adm,
                               int32_t* out_mode);

}  // namespace livekit_ffi
