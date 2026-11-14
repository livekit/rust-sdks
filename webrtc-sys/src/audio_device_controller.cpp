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

#include "livekit/audio_device_controller.h"

#include <cstdint>
#include <optional>
#include <string>
#include <utility>

namespace livekit_ffi {

namespace {

using WindowsDeviceType = webrtc::AudioDeviceModule::WindowsDeviceType;

constexpr char kWindowsDeviceTypePrefix[] = "windows:";
constexpr char kIndexPrefix[] = "index:";

// Parses the number following `prefix` in `token`.
std::optional<int> ParseSelectionNumber(const std::string& token,
                                        const std::string& prefix) {
  if (token.rfind(prefix, 0) != 0) {
    return std::nullopt;
  }
  try {
    size_t pos = 0;
    int value = std::stoi(token.substr(prefix.size()), &pos);
    if (pos == token.size() - prefix.size()) {
      return value;
    }
  } catch (...) {
  }
  return std::nullopt;
}

// Selects the platform default device, which a token of "" stands for.
template <typename SetByIndex, typename SetByType>
bool SelectPlatformDefault(SetByIndex set_by_index, SetByType set_by_type) {
#if defined(WEBRTC_WIN)
  // WebRTC's Windows ADM uses the default communication device until a device
  // is selected.
  return set_by_type(webrtc::AudioDeviceModule::kDefaultCommunicationDevice) ==
         0;
#else
  return set_by_index(0) == 0;
#endif
}

}  // namespace

AudioDeviceController::AudioDeviceController(
    std::shared_ptr<RtcRuntime> rtc_runtime,
    webrtc::scoped_refptr<AdmProxy> adm_proxy)
    : rtc_runtime_(std::move(rtc_runtime)), adm_proxy_(std::move(adm_proxy)) {}

int16_t AudioDeviceController::playout_devices() const {
  return adm_proxy_->PlayoutDevices();
}

int16_t AudioDeviceController::recording_devices() const {
  return adm_proxy_->RecordingDevices();
}

rust::String AudioDeviceController::playout_device_name(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->PlayoutDeviceName(index, name, guid);
  return rust::String(name);
}

rust::String AudioDeviceController::recording_device_name(
    uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->RecordingDeviceName(index, name, guid);
  return rust::String(name);
}

rust::String AudioDeviceController::playout_device_guid(uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->PlayoutDeviceName(index, name, guid);
  if (guid[0] != '\0') {
    return rust::String(guid);
  }
  // Linux PulseAudio/ALSA does not populate device GUIDs in WebRTC.
  // Synthesize an index-based identifier so callers can distinguish and select
  // devices.
  return rust::String("index:" + std::to_string(index));
}

rust::String AudioDeviceController::recording_device_guid(
    uint16_t index) const {
  char name[webrtc::kAdmMaxDeviceNameSize] = {0};
  char guid[webrtc::kAdmMaxGuidSize] = {0};
  adm_proxy_->RecordingDeviceName(index, name, guid);
  if (guid[0] != '\0') {
    return rust::String(guid);
  }
  // Linux PulseAudio/ALSA does not populate device GUIDs in WebRTC.
  // Synthesize an index-based identifier so callers can distinguish and select
  // devices.
  return rust::String("index:" + std::to_string(index));
}

bool AudioDeviceController::set_playout_device(uint16_t index) const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return select_playout_device(index);
}

bool AudioDeviceController::set_playout_device_by_guid(
    rust::String guid) const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return select_playout_device_by_id(std::string(guid.c_str()));
}

rust::String AudioDeviceController::playout_device_selection() const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return rust::String(playout_selection_);
}

bool AudioDeviceController::restore_playout_device_selection(
    rust::String selection) const {
  std::string token(selection.c_str());
  std::lock_guard<std::mutex> lock(selection_mutex_);
  if (token.empty()) {
    bool selected = SelectPlatformDefault(
        [this](uint16_t index) { return adm_proxy_->SetPlayoutDevice(index); },
        [this](WindowsDeviceType type) {
          return adm_proxy_->SetPlayoutDevice(type);
        });
    if (selected) {
      playout_selection_.clear();
    }
    return selected;
  }
  if (auto type = ParseSelectionNumber(token, kWindowsDeviceTypePrefix)) {
    if (adm_proxy_->SetPlayoutDevice(static_cast<WindowsDeviceType>(*type)) !=
        0) {
      return false;
    }
    playout_selection_ = std::move(token);
    return true;
  }
  if (auto index = ParseSelectionNumber(token, kIndexPrefix)) {
    // Selected directly, as set_playout_device(index) does: before the platform
    // ADM exists there is no device list to check the index against.
    return *index >= 0 && *index <= UINT16_MAX &&
           select_playout_device(static_cast<uint16_t>(*index));
  }
  // A device GUID: find the device in the current list, which may have changed
  // since it was selected.
  return select_playout_device_by_id(token);
}

bool AudioDeviceController::select_playout_device(uint16_t index) const {
  if (adm_proxy_->SetPlayoutDevice(index) != 0) {
    return false;
  }
  playout_selection_ = std::string(playout_device_guid(index).c_str());
  return true;
}

bool AudioDeviceController::select_playout_device_by_id(
    const std::string& target_guid) const {
  int16_t count = adm_proxy_->PlayoutDevices();

  if (target_guid.empty()) {
    return false;
  }

  // Handle synthetic index-based GUIDs (e.g. "index:1")
  if (target_guid.rfind("index:", 0) == 0) {
    try {
      size_t pos = 0;
      int idx = std::stoi(target_guid.substr(6), &pos);
      if (pos == target_guid.length() - 6 && idx >= 0 && idx < count) {
        return select_playout_device(static_cast<uint16_t>(idx));
      }
    } catch (...) {
    }
    return false;
  }

  // Try to find a device matching the GUID (only match non-empty device GUIDs)
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->PlayoutDeviceName(i, name, device_guid) == 0) {
      if (device_guid[0] != '\0' && target_guid == std::string(device_guid)) {
        return select_playout_device(i);
      }
    }
  }

  // Fallback: match by device name if no GUID matched
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->PlayoutDeviceName(i, name, device_guid) == 0) {
      if (target_guid == std::string(name)) {
        return select_playout_device(i);
      }
    }
  }

#if defined(WEBRTC_IOS) || defined(WEBRTC_ANDROID)
  // On mobile platforms, device selection is a no-op (system handles routing),
  // so fall back to default device (index 0).
  if (count > 0) {
    return select_playout_device(0);
  }
#endif

  return false;
}

bool AudioDeviceController::set_recording_device(uint16_t index) const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return select_recording_device(index);
}

bool AudioDeviceController::set_recording_device_by_guid(
    rust::String guid) const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return select_recording_device_by_id(std::string(guid.c_str()));
}

rust::String AudioDeviceController::recording_device_selection() const {
  std::lock_guard<std::mutex> lock(selection_mutex_);
  return rust::String(recording_selection_);
}

bool AudioDeviceController::restore_recording_device_selection(
    rust::String selection) const {
  std::string token(selection.c_str());
  std::lock_guard<std::mutex> lock(selection_mutex_);
  if (token.empty()) {
    bool selected = SelectPlatformDefault(
        [this](uint16_t index) {
          return adm_proxy_->SetRecordingDevice(index);
        },
        [this](WindowsDeviceType type) {
          return adm_proxy_->SetRecordingDevice(type);
        });
    if (selected) {
      recording_selection_.clear();
    }
    return selected;
  }
  if (auto type = ParseSelectionNumber(token, kWindowsDeviceTypePrefix)) {
    if (adm_proxy_->SetRecordingDevice(static_cast<WindowsDeviceType>(*type)) !=
        0) {
      return false;
    }
    recording_selection_ = std::move(token);
    return true;
  }
  if (auto index = ParseSelectionNumber(token, kIndexPrefix)) {
    // Selected directly, as set_recording_device(index) does: before the
    // platform ADM exists there is no device list to check the index against.
    return *index >= 0 && *index <= UINT16_MAX &&
           select_recording_device(static_cast<uint16_t>(*index));
  }
  // A device GUID: find the device in the current list, which may have changed
  // since it was selected.
  return select_recording_device_by_id(token);
}

bool AudioDeviceController::select_recording_device(uint16_t index) const {
  if (adm_proxy_->SetRecordingDevice(index) != 0) {
    return false;
  }
  recording_selection_ = std::string(recording_device_guid(index).c_str());
  return true;
}

bool AudioDeviceController::select_recording_device_by_id(
    const std::string& target_guid) const {
  int16_t count = adm_proxy_->RecordingDevices();

  if (target_guid.empty()) {
    return false;
  }

  // Handle synthetic index-based GUIDs (e.g. "index:1")
  if (target_guid.rfind("index:", 0) == 0) {
    try {
      size_t pos = 0;
      int idx = std::stoi(target_guid.substr(6), &pos);
      if (pos == target_guid.length() - 6 && idx >= 0 && idx < count) {
        return select_recording_device(static_cast<uint16_t>(idx));
      }
    } catch (...) {
    }
    return false;
  }

  // Try to find a device matching the GUID (only match non-empty device GUIDs)
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->RecordingDeviceName(i, name, device_guid) == 0) {
      if (device_guid[0] != '\0' && target_guid == std::string(device_guid)) {
        return select_recording_device(i);
      }
    }
  }

  // Fallback: match by device name if no GUID matched
  for (int16_t i = 0; i < count; i++) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char device_guid[webrtc::kAdmMaxGuidSize] = {0};
    if (adm_proxy_->RecordingDeviceName(i, name, device_guid) == 0) {
      if (target_guid == std::string(name)) {
        return select_recording_device(i);
      }
    }
  }

#if defined(WEBRTC_IOS) || defined(WEBRTC_ANDROID)
  // On mobile platforms, device selection is a no-op (system handles routing),
  // so fall back to default device (index 0).
  if (count > 0) {
    return select_recording_device(0);
  }
#endif

  return false;
}

bool AudioDeviceController::stop_recording() const {
  return adm_proxy_->StopRecording() == 0;
}

bool AudioDeviceController::init_recording() const {
  return adm_proxy_->InitRecording() == 0;
}

bool AudioDeviceController::start_recording() const {
  return adm_proxy_->StartRecording() == 0;
}

bool AudioDeviceController::recording_is_initialized() const {
  return adm_proxy_->RecordingIsInitialized();
}

bool AudioDeviceController::stop_playout() const {
  return adm_proxy_->StopPlayout() == 0;
}

bool AudioDeviceController::init_playout() const {
  return adm_proxy_->InitPlayout() == 0;
}

bool AudioDeviceController::start_playout() const {
  return adm_proxy_->StartPlayout() == 0;
}

bool AudioDeviceController::playout_is_initialized() const {
  return adm_proxy_->PlayoutIsInitialized();
}

bool AudioDeviceController::builtin_aec_is_available() const {
  return adm_proxy_->BuiltInAECIsAvailable();
}

bool AudioDeviceController::builtin_agc_is_available() const {
  return adm_proxy_->BuiltInAGCIsAvailable();
}

bool AudioDeviceController::builtin_ns_is_available() const {
  return adm_proxy_->BuiltInNSIsAvailable();
}

bool AudioDeviceController::enable_builtin_aec(bool enable) const {
  return adm_proxy_->EnableBuiltInAEC(enable) == 0;
}

bool AudioDeviceController::enable_builtin_agc(bool enable) const {
  return adm_proxy_->EnableBuiltInAGC(enable) == 0;
}

bool AudioDeviceController::enable_builtin_ns(bool enable) const {
  return adm_proxy_->EnableBuiltInNS(enable) == 0;
}

void AudioDeviceController::set_adm_recording_enabled(bool enabled) const {
  adm_proxy_->set_recording_enabled(enabled);
}

bool AudioDeviceController::adm_recording_enabled() const {
  return adm_proxy_->recording_enabled();
}

void AudioDeviceController::set_adm_playout_enabled(bool enabled) const {
  adm_proxy_->set_playout_enabled(enabled);
}

bool AudioDeviceController::adm_playout_enabled() const {
  return adm_proxy_->playout_enabled();
}

bool AudioDeviceController::acquire_platform_adm() const {
  return adm_proxy_->AcquirePlatformAdm();
}

void AudioDeviceController::release_platform_adm() const {
  adm_proxy_->ReleasePlatformAdm();
}

int AudioDeviceController::platform_adm_ref_count() const {
  return adm_proxy_->platform_adm_ref_count();
}

bool AudioDeviceController::is_platform_adm_active() const {
  return adm_proxy_->is_platform_adm_active();
}

}  // namespace livekit_ffi
