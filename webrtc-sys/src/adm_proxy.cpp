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

#include "livekit/adm_proxy.h"

#include "api/audio/audio_device.h"
#include "api/audio/create_audio_device_module.h"
#include "rtc_base/logging.h"
#include "rtc_base/thread.h"

namespace livekit_ffi {

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env), worker_thread_(worker_thread) {
  // Create the platform ADM
  platform_adm_ = webrtc::CreateAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to create Platform ADM";
    return;
  }

  // Initialize the platform ADM
  int32_t init_result = platform_adm_->Init();
  if (init_result != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to initialize Platform ADM, error=" << init_result;
    platform_adm_ = nullptr;
    return;
  }

  adm_initialized_ = true;
  RTC_LOG(LS_INFO) << "AdmProxy: Platform ADM initialized, "
                   << platform_adm_->RecordingDevices() << " recording devices, "
                   << platform_adm_->PlayoutDevices() << " playout devices";
}

AdmProxy::~AdmProxy() {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::~AdmProxy()";
  if (platform_adm_) {
    platform_adm_->Terminate();
    platform_adm_ = nullptr;
  }
}

bool AdmProxy::is_initialized() const {
  return adm_initialized_;
}

void AdmProxy::set_recording_enabled(bool enabled) {
  RTC_LOG(LS_INFO) << "AdmProxy::set_recording_enabled(" << enabled << ")";
  recording_enabled_ = enabled;
}

bool AdmProxy::recording_enabled() const {
  return recording_enabled_;
}

// AudioDeviceModule interface - delegate all calls to platform_adm_

int32_t AdmProxy::ActiveAudioLayer(AudioLayer* audioLayer) const {
  if (!platform_adm_) {
    *audioLayer = AudioLayer::kDummyAudio;
    return 0;
  }
  return platform_adm_->ActiveAudioLayer(audioLayer);
}

int32_t AdmProxy::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  if (!platform_adm_) return -1;
  return platform_adm_->RegisterAudioCallback(transport);
}

int32_t AdmProxy::Init() {
  // Already initialized in constructor
  if (!platform_adm_) return -1;
  return 0;
}

int32_t AdmProxy::Terminate() {
  if (!platform_adm_) return 0;
  adm_initialized_ = false;
  return platform_adm_->Terminate();
}

bool AdmProxy::Initialized() const {
  if (!platform_adm_) return false;
  return platform_adm_->Initialized();
}

int16_t AdmProxy::PlayoutDevices() {
  if (!platform_adm_) return 0;
  return platform_adm_->PlayoutDevices();
}

int16_t AdmProxy::RecordingDevices() {
  if (!platform_adm_) return 0;
  return platform_adm_->RecordingDevices();
}

int32_t AdmProxy::PlayoutDeviceName(uint16_t index,
                                    char name[webrtc::kAdmMaxDeviceNameSize],
                                    char guid[webrtc::kAdmMaxGuidSize]) {
  if (!platform_adm_) return -1;
  return platform_adm_->PlayoutDeviceName(index, name, guid);
}

int32_t AdmProxy::RecordingDeviceName(uint16_t index,
                                      char name[webrtc::kAdmMaxDeviceNameSize],
                                      char guid[webrtc::kAdmMaxGuidSize]) {
  if (!platform_adm_) return -1;
  return platform_adm_->RecordingDeviceName(index, name, guid);
}

int32_t AdmProxy::SetPlayoutDevice(uint16_t index) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetPlayoutDevice(index);
}

int32_t AdmProxy::SetPlayoutDevice(WindowsDeviceType device) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetPlayoutDevice(device);
}

int32_t AdmProxy::SetRecordingDevice(uint16_t index) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetRecordingDevice(index);
}

int32_t AdmProxy::SetRecordingDevice(WindowsDeviceType device) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetRecordingDevice(device);
}

int32_t AdmProxy::PlayoutIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->PlayoutIsAvailable(available);
}

int32_t AdmProxy::InitPlayout() {
  if (!platform_adm_) return -1;
  return platform_adm_->InitPlayout();
}

bool AdmProxy::PlayoutIsInitialized() const {
  if (!platform_adm_) return false;
  return platform_adm_->PlayoutIsInitialized();
}

int32_t AdmProxy::RecordingIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->RecordingIsAvailable(available);
}

int32_t AdmProxy::InitRecording() {
  if (!platform_adm_) return -1;
  if (!recording_enabled_) {
    return 0;  // Return success but don't actually initialize
  }
  return platform_adm_->InitRecording();
}

bool AdmProxy::RecordingIsInitialized() const {
  if (!platform_adm_) return false;
  if (!recording_enabled_) return false;
  return platform_adm_->RecordingIsInitialized();
}

int32_t AdmProxy::StartPlayout() {
  if (!platform_adm_) return -1;
  return platform_adm_->StartPlayout();
}

int32_t AdmProxy::StopPlayout() {
  if (!platform_adm_) return 0;
  return platform_adm_->StopPlayout();
}

bool AdmProxy::Playing() const {
  if (!platform_adm_) return false;
  return platform_adm_->Playing();
}

int32_t AdmProxy::StartRecording() {
  if (!platform_adm_) return -1;
  if (!recording_enabled_) {
    return 0;  // Return success but don't actually start
  }
  return platform_adm_->StartRecording();
}

int32_t AdmProxy::StopRecording() {
  if (!platform_adm_) return 0;
  return platform_adm_->StopRecording();
}

bool AdmProxy::Recording() const {
  if (!platform_adm_) return false;
  if (!recording_enabled_) return false;
  return platform_adm_->Recording();
}

int32_t AdmProxy::InitSpeaker() {
  if (!platform_adm_) return -1;
  return platform_adm_->InitSpeaker();
}

bool AdmProxy::SpeakerIsInitialized() const {
  if (!platform_adm_) return false;
  return platform_adm_->SpeakerIsInitialized();
}

int32_t AdmProxy::InitMicrophone() {
  if (!platform_adm_) return -1;
  return platform_adm_->InitMicrophone();
}

bool AdmProxy::MicrophoneIsInitialized() const {
  if (!platform_adm_) return false;
  return platform_adm_->MicrophoneIsInitialized();
}

int32_t AdmProxy::SpeakerVolumeIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->SpeakerVolumeIsAvailable(available);
}

int32_t AdmProxy::SetSpeakerVolume(uint32_t volume) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetSpeakerVolume(volume);
}

int32_t AdmProxy::SpeakerVolume(uint32_t* volume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->SpeakerVolume(volume);
}

int32_t AdmProxy::MaxSpeakerVolume(uint32_t* maxVolume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MaxSpeakerVolume(maxVolume);
}

int32_t AdmProxy::MinSpeakerVolume(uint32_t* minVolume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MinSpeakerVolume(minVolume);
}

int32_t AdmProxy::MicrophoneVolumeIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->MicrophoneVolumeIsAvailable(available);
}

int32_t AdmProxy::SetMicrophoneVolume(uint32_t volume) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetMicrophoneVolume(volume);
}

int32_t AdmProxy::MicrophoneVolume(uint32_t* volume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MicrophoneVolume(volume);
}

int32_t AdmProxy::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MaxMicrophoneVolume(maxVolume);
}

int32_t AdmProxy::MinMicrophoneVolume(uint32_t* minVolume) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MinMicrophoneVolume(minVolume);
}

int32_t AdmProxy::SpeakerMuteIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->SpeakerMuteIsAvailable(available);
}

int32_t AdmProxy::SetSpeakerMute(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetSpeakerMute(enable);
}

int32_t AdmProxy::SpeakerMute(bool* enabled) const {
  if (!platform_adm_) return -1;
  return platform_adm_->SpeakerMute(enabled);
}

int32_t AdmProxy::MicrophoneMuteIsAvailable(bool* available) {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->MicrophoneMuteIsAvailable(available);
}

int32_t AdmProxy::SetMicrophoneMute(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetMicrophoneMute(enable);
}

int32_t AdmProxy::MicrophoneMute(bool* enabled) const {
  if (!platform_adm_) return -1;
  return platform_adm_->MicrophoneMute(enabled);
}

int32_t AdmProxy::StereoPlayoutIsAvailable(bool* available) const {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->StereoPlayoutIsAvailable(available);
}

int32_t AdmProxy::SetStereoPlayout(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetStereoPlayout(enable);
}

int32_t AdmProxy::StereoPlayout(bool* enabled) const {
  if (!platform_adm_) return -1;
  return platform_adm_->StereoPlayout(enabled);
}

int32_t AdmProxy::StereoRecordingIsAvailable(bool* available) const {
  if (!platform_adm_) {
    *available = false;
    return 0;
  }
  return platform_adm_->StereoRecordingIsAvailable(available);
}

int32_t AdmProxy::SetStereoRecording(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetStereoRecording(enable);
}

int32_t AdmProxy::StereoRecording(bool* enabled) const {
  if (!platform_adm_) return -1;
  return platform_adm_->StereoRecording(enabled);
}

int32_t AdmProxy::PlayoutDelay(uint16_t* delayMS) const {
  if (!platform_adm_) {
    *delayMS = 0;
    return 0;
  }
  return platform_adm_->PlayoutDelay(delayMS);
}

bool AdmProxy::BuiltInAECIsAvailable() const {
  if (!platform_adm_) return false;
  return platform_adm_->BuiltInAECIsAvailable();
}

bool AdmProxy::BuiltInAGCIsAvailable() const {
  if (!platform_adm_) return false;
  return platform_adm_->BuiltInAGCIsAvailable();
}

bool AdmProxy::BuiltInNSIsAvailable() const {
  if (!platform_adm_) return false;
  return platform_adm_->BuiltInNSIsAvailable();
}

int32_t AdmProxy::EnableBuiltInAEC(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->EnableBuiltInAEC(enable);
}

int32_t AdmProxy::EnableBuiltInAGC(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->EnableBuiltInAGC(enable);
}

int32_t AdmProxy::EnableBuiltInNS(bool enable) {
  if (!platform_adm_) return -1;
  return platform_adm_->EnableBuiltInNS(enable);
}

#if defined(WEBRTC_IOS)
int AdmProxy::GetPlayoutAudioParameters(webrtc::AudioParameters* params) const {
  if (!platform_adm_) return -1;
  return platform_adm_->GetPlayoutAudioParameters(params);
}

int AdmProxy::GetRecordAudioParameters(webrtc::AudioParameters* params) const {
  if (!platform_adm_) return -1;
  return platform_adm_->GetRecordAudioParameters(params);
}
#endif

int32_t AdmProxy::SetObserver(webrtc::AudioDeviceObserver* observer) {
  if (!platform_adm_) return -1;
  return platform_adm_->SetObserver(observer);
}

}  // namespace livekit_ffi
