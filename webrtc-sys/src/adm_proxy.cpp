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
#include "api/task_queue/pending_task_safety_flag.h"
#include "rtc_base/logging.h"
#include "rtc_base/thread.h"
#include "rtc_base/time_utils.h"

namespace {
constexpr int kSampleRate = 48000;
constexpr int kChannels = 2;
constexpr int kSamplesPer10Ms = kSampleRate / 100;
}  // namespace

namespace livekit_ffi {

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env),
      worker_thread_(worker_thread),
      stub_data_(kSamplesPer10Ms * kChannels) {
  RTC_LOG(LS_INFO) << "AdmProxy::AdmProxy() - Lazy initialization mode (no Platform ADM yet)";
}

AdmProxy::~AdmProxy() {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::~AdmProxy()";
  StopSyntheticPlayoutTask();
  if (platform_adm_) {
    platform_adm_->Terminate();
    platform_adm_ = nullptr;
  }
}

// =============================================================================
// Platform ADM Lifecycle Management
// =============================================================================

bool AdmProxy::AcquirePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);

  platform_adm_ref_count_++;
  RTC_LOG(LS_INFO) << "AdmProxy::AcquirePlatformAdm() ref_count=" << platform_adm_ref_count_;

  if (platform_adm_ref_count_ == 1) {
    // First user - create and initialize Platform ADM
    if (!CreatePlatformAdm()) {
      platform_adm_ref_count_--;
      return false;
    }
  }

  return platform_adm_ != nullptr;
}

void AdmProxy::ReleasePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);

  if (platform_adm_ref_count_ <= 0) {
    RTC_LOG(LS_WARNING) << "AdmProxy::ReleasePlatformAdm() called with ref_count="
                        << platform_adm_ref_count_;
    return;
  }

  platform_adm_ref_count_--;
  RTC_LOG(LS_INFO) << "AdmProxy::ReleasePlatformAdm() ref_count=" << platform_adm_ref_count_;

  if (platform_adm_ref_count_ == 0) {
    // Last user - terminate Platform ADM and return to synthetic mode
    TerminatePlatformAdm();
  }
}

int AdmProxy::platform_adm_ref_count() const {
  webrtc::MutexLock lock(&mutex_);
  return platform_adm_ref_count_;
}

bool AdmProxy::is_platform_adm_active() const {
  webrtc::MutexLock lock(&mutex_);
  return platform_adm_ref_count_ > 0 && platform_adm_ != nullptr;
}

bool AdmProxy::CreatePlatformAdm() {
  // Note: Called with mutex_ held
  RTC_LOG(LS_INFO) << "AdmProxy::CreatePlatformAdm() - Creating Platform ADM";

  platform_adm_ = webrtc::CreateAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to create Platform ADM";
    return false;
  }

  int32_t init_result = platform_adm_->Init();
  if (init_result != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to initialize Platform ADM, error=" << init_result;
    platform_adm_ = nullptr;
    return false;
  }

  // Register the audio transport callback if we have one
  if (audio_transport_) {
    platform_adm_->RegisterAudioCallback(audio_transport_);
  }

  int16_t num_recording = platform_adm_->RecordingDevices();
  int16_t num_playout = platform_adm_->PlayoutDevices();

  RTC_LOG(LS_INFO) << "AdmProxy: Platform ADM initialized, "
                   << num_recording << " recording devices, "
                   << num_playout << " playout devices";

  // Restore selected recording device - prefer GUID, fall back to index with validation
  if (!selected_recording_guid_.empty()) {
    // Try to find device by GUID
    bool found = false;
    for (int16_t i = 0; i < num_recording; i++) {
      char name[webrtc::kAdmMaxDeviceNameSize] = {0};
      char guid[webrtc::kAdmMaxGuidSize] = {0};
      if (platform_adm_->RecordingDeviceName(i, name, guid) == 0) {
        if (selected_recording_guid_ == guid) {
          platform_adm_->SetRecordingDevice(i);
          selected_recording_device_ = i;  // Update index to match
          RTC_LOG(LS_INFO) << "AdmProxy: Restored recording device by GUID: " << name;
          found = true;
          break;
        }
      }
    }
    if (!found) {
      RTC_LOG(LS_WARNING) << "AdmProxy: Previously selected recording device GUID not found, using default";
      selected_recording_device_ = 0;
      selected_recording_guid_.clear();
    }
  } else if (selected_recording_device_ > 0) {
    // Fall back to index-based restoration with validation
    if (selected_recording_device_ < num_recording) {
      platform_adm_->SetRecordingDevice(selected_recording_device_);
      RTC_LOG(LS_INFO) << "AdmProxy: Restored recording device by index: " << selected_recording_device_;
    } else {
      RTC_LOG(LS_WARNING) << "AdmProxy: Previously selected recording device index "
                          << selected_recording_device_ << " invalid (only "
                          << num_recording << " devices), using default";
      selected_recording_device_ = 0;
    }
  }

  // Restore selected playout device - prefer GUID, fall back to index with validation
  if (!selected_playout_guid_.empty()) {
    // Try to find device by GUID
    bool found = false;
    for (int16_t i = 0; i < num_playout; i++) {
      char name[webrtc::kAdmMaxDeviceNameSize] = {0};
      char guid[webrtc::kAdmMaxGuidSize] = {0};
      if (platform_adm_->PlayoutDeviceName(i, name, guid) == 0) {
        if (selected_playout_guid_ == guid) {
          platform_adm_->SetPlayoutDevice(i);
          selected_playout_device_ = i;  // Update index to match
          RTC_LOG(LS_INFO) << "AdmProxy: Restored playout device by GUID: " << name;
          found = true;
          break;
        }
      }
    }
    if (!found) {
      RTC_LOG(LS_WARNING) << "AdmProxy: Previously selected playout device GUID not found, using default";
      selected_playout_device_ = 0;
      selected_playout_guid_.clear();
    }
  } else if (selected_playout_device_ > 0) {
    // Fall back to index-based restoration with validation
    if (selected_playout_device_ < num_playout) {
      platform_adm_->SetPlayoutDevice(selected_playout_device_);
      RTC_LOG(LS_INFO) << "AdmProxy: Restored playout device by index: " << selected_playout_device_;
    } else {
      RTC_LOG(LS_WARNING) << "AdmProxy: Previously selected playout device index "
                          << selected_playout_device_ << " invalid (only "
                          << num_playout << " devices), using default";
      selected_playout_device_ = 0;
    }
  }

  return true;
}

void AdmProxy::TerminatePlatformAdm() {
  // Note: Called with mutex_ held
  RTC_LOG(LS_INFO) << "AdmProxy::TerminatePlatformAdm() - Returning to synthetic mode";

  if (!platform_adm_) {
    return;
  }

  // Stop any active recording/playout
  if (recording_) {
    platform_adm_->StopRecording();
    recording_ = false;
  }
  if (playing_) {
    platform_adm_->StopPlayout();
    playing_ = false;
  }

  platform_adm_->RegisterAudioCallback(nullptr);
  platform_adm_->Terminate();
  platform_adm_ = nullptr;

  playout_initialized_ = false;
  recording_initialized_ = false;

  // Reset control flags to defaults
  recording_enabled_ = false;
  playout_enabled_ = false;

  RTC_LOG(LS_INFO) << "AdmProxy: Platform ADM terminated, now in synthetic mode";
}

// =============================================================================
// Recording/Playout Control
// =============================================================================

void AdmProxy::set_recording_enabled(bool enabled) {
  webrtc::MutexLock lock(&mutex_);
  RTC_LOG(LS_INFO) << "AdmProxy::set_recording_enabled(" << enabled << ")";
  recording_enabled_ = enabled;
}

bool AdmProxy::recording_enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return recording_enabled_;
}

void AdmProxy::set_playout_enabled(bool enabled) {
  webrtc::MutexLock lock(&mutex_);
  RTC_LOG(LS_INFO) << "AdmProxy::set_playout_enabled(" << enabled << ")";
  playout_enabled_ = enabled;
}

bool AdmProxy::playout_enabled() const {
  webrtc::MutexLock lock(&mutex_);
  return playout_enabled_;
}

// =============================================================================
// Synthetic Playout Task
// =============================================================================

void AdmProxy::StartSyntheticPlayoutTask() {
  // Note: This creates a task that periodically pulls audio to keep
  // WebRTC's audio pipeline alive. The audio is discarded (not played).
  //
  // This is essential for:
  // - Keeping WebRTC's audio decoder running (needed for FFI callbacks)
  // - Maintaining audio sync timestamps
  // - Allowing NativeAudioStream to receive remote audio frames
  //
  // The actual audio playback happens in the FFI client (e.g., Unity AudioSource).

  if (stub_audio_queue_) {
    return;  // Already running
  }

  RTC_LOG(LS_INFO) << "AdmProxy: Starting synthetic playout task";

  stub_audio_queue_ = env_.task_queue_factory().CreateTaskQueue(
      "AdmProxySyntheticPlayout", webrtc::TaskQueueFactory::Priority::HIGH);

  stub_audio_task_ = webrtc::RepeatingTaskHandle::Start(
      stub_audio_queue_.get(),
      [this]() {
        webrtc::AudioTransport* transport = nullptr;
        bool should_run = false;
        {
          webrtc::MutexLock lock(&mutex_);
          should_run = playing_ && !playout_enabled_;
          transport = audio_transport_;
        }

        if (should_run && transport) {
          int64_t elapsed_time_ms = -1;
          int64_t ntp_time_ms = -1;
          size_t n_samples_out = 0;

          // Pull audio from WebRTC (this keeps the audio pipeline alive)
          // Audio is discarded - FFI callbacks deliver it to the application
          transport->NeedMorePlayData(
              kSamplesPer10Ms,    // samples per channel
              sizeof(int16_t),    // bytes per sample
              kChannels,          // channels
              kSampleRate,        // sample rate
              stub_data_.data(),  // output buffer (discarded)
              n_samples_out,
              &elapsed_time_ms,
              &ntp_time_ms);
        }

        return webrtc::TimeDelta::Millis(10);  // Run every 10ms
      });
}

void AdmProxy::StopSyntheticPlayoutTask() {
  if (stub_audio_queue_) {
    RTC_LOG(LS_INFO) << "AdmProxy: Stopping synthetic playout task";
    stub_audio_task_.Stop();
    stub_audio_queue_.reset();
  }
}

// =============================================================================
// AudioDeviceModule Interface Implementation
// =============================================================================

int32_t AdmProxy::ActiveAudioLayer(AudioLayer* audioLayer) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->ActiveAudioLayer(audioLayer);
  }
  *audioLayer = AudioLayer::kDummyAudio;
  return 0;
}

int32_t AdmProxy::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  webrtc::MutexLock lock(&mutex_);
  audio_transport_ = transport;
  if (platform_adm_) {
    return platform_adm_->RegisterAudioCallback(transport);
  }
  return 0;
}

int32_t AdmProxy::Init() {
  // Init is a no-op - Platform ADM is created lazily via AcquirePlatformAdm()
  return 0;
}

int32_t AdmProxy::Terminate() {
  webrtc::MutexLock lock(&mutex_);
  StopSyntheticPlayoutTask();
  if (platform_adm_) {
    return platform_adm_->Terminate();
  }
  return 0;
}

bool AdmProxy::Initialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->Initialized();
  }
  return true;  // Synthetic mode is always "initialized"
}

int16_t AdmProxy::PlayoutDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutDevices();
  }
  // In synthetic mode, return 0 devices (no platform audio)
  return 0;
}

int16_t AdmProxy::RecordingDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingDevices();
  }
  // In synthetic mode, return 0 devices (no platform audio)
  return 0;
}

int32_t AdmProxy::PlayoutDeviceName(uint16_t index,
                                    char name[webrtc::kAdmMaxDeviceNameSize],
                                    char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::RecordingDeviceName(uint16_t index,
                                      char name[webrtc::kAdmMaxDeviceNameSize],
                                      char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::SetPlayoutDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  selected_playout_device_ = index;

  // Also store the GUID for this device for robust restoration
  if (platform_adm_) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char guid[webrtc::kAdmMaxGuidSize] = {0};
    if (platform_adm_->PlayoutDeviceName(index, name, guid) == 0) {
      selected_playout_guid_ = guid;
    }
    return platform_adm_->SetPlayoutDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetPlayoutDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  // Note: When using WindowsDeviceType, we can't easily get the GUID
  // The GUID will be populated on next CreatePlatformAdm if needed
  selected_playout_guid_.clear();
  if (platform_adm_) {
    return platform_adm_->SetPlayoutDevice(device);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  selected_recording_device_ = index;

  // Also store the GUID for this device for robust restoration
  if (platform_adm_) {
    char name[webrtc::kAdmMaxDeviceNameSize] = {0};
    char guid[webrtc::kAdmMaxGuidSize] = {0};
    if (platform_adm_->RecordingDeviceName(index, name, guid) == 0) {
      selected_recording_guid_ = guid;
    }
    return platform_adm_->SetRecordingDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  // Note: When using WindowsDeviceType, we can't easily get the GUID
  // The GUID will be populated on next CreatePlatformAdm if needed
  selected_recording_guid_.clear();
  if (platform_adm_) {
    return platform_adm_->SetRecordingDevice(device);
  }
  return 0;
}

int32_t AdmProxy::PlayoutIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutIsAvailable(available);
  }
  *available = true;  // Synthetic playout is always available
  return 0;
}

int32_t AdmProxy::InitPlayout() {
  webrtc::MutexLock lock(&mutex_);

  if (platform_adm_ && playout_enabled_) {
    int32_t result = platform_adm_->InitPlayout();
    if (result == 0) {
      playout_initialized_ = true;
    }
    return result;
  }

  // Synthetic mode - mark as initialized
  playout_initialized_ = true;
  return 0;
}

bool AdmProxy::PlayoutIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_ && playout_enabled_) {
    return platform_adm_->PlayoutIsInitialized();
  }
  return playout_initialized_;
}

int32_t AdmProxy::RecordingIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->RecordingIsAvailable(available);
  }
  *available = false;  // Recording not available in synthetic mode
  return 0;
}

int32_t AdmProxy::InitRecording() {
  webrtc::MutexLock lock(&mutex_);

  if (!recording_enabled_) {
    // Recording disabled - return success but don't initialize
    return 0;
  }

  if (platform_adm_) {
    int32_t result = platform_adm_->InitRecording();
    if (result == 0) {
      recording_initialized_ = true;
    }
    return result;
  }

  return -1;  // No Platform ADM and recording is enabled = error
}

bool AdmProxy::RecordingIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (!recording_enabled_) {
    return false;
  }
  if (platform_adm_) {
    return platform_adm_->RecordingIsInitialized();
  }
  return recording_initialized_;
}

int32_t AdmProxy::StartPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = true;

  if (platform_adm_ && playout_enabled_) {
    // Platform mode - use real speakers
    return platform_adm_->StartPlayout();
  }

  // Synthetic mode - start task to pull audio (keeps pipeline alive)
  // Must release mutex before starting task to avoid deadlock
  mutex_.Unlock();
  StartSyntheticPlayoutTask();
  mutex_.Lock();
  return 0;
}

int32_t AdmProxy::StopPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = false;

  // Stop synthetic task first (must release mutex)
  mutex_.Unlock();
  StopSyntheticPlayoutTask();
  mutex_.Lock();

  if (platform_adm_ && playout_enabled_) {
    return platform_adm_->StopPlayout();
  }
  return 0;
}

bool AdmProxy::Playing() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_ && playout_enabled_) {
    return platform_adm_->Playing();
  }
  return playing_;
}

int32_t AdmProxy::StartRecording() {
  webrtc::MutexLock lock(&mutex_);

  if (!recording_enabled_) {
    // Recording disabled - return success but don't start
    return 0;
  }

  recording_ = true;

  if (platform_adm_) {
    int32_t result = platform_adm_->StartRecording();
    RTC_LOG(LS_INFO) << "AdmProxy::StartRecording() platform_adm result=" << result;
    return result;
  }

  RTC_LOG(LS_WARNING) << "AdmProxy::StartRecording() called but no Platform ADM!";
  return -1;
}

int32_t AdmProxy::StopRecording() {
  webrtc::MutexLock lock(&mutex_);
  recording_ = false;

  if (platform_adm_) {
    return platform_adm_->StopRecording();
  }
  return 0;
}

bool AdmProxy::Recording() const {
  webrtc::MutexLock lock(&mutex_);
  if (!recording_enabled_) {
    return false;
  }
  if (platform_adm_) {
    return platform_adm_->Recording();
  }
  return recording_;
}

int32_t AdmProxy::InitSpeaker() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->InitSpeaker();
  }
  return 0;
}

bool AdmProxy::SpeakerIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerIsInitialized();
  }
  return true;
}

int32_t AdmProxy::InitMicrophone() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->InitMicrophone();
  }
  return 0;
}

bool AdmProxy::MicrophoneIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneIsInitialized();
  }
  return false;
}

int32_t AdmProxy::SpeakerVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetSpeakerVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::SpeakerVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MaxSpeakerVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MaxSpeakerVolume(maxVolume);
  }
  return -1;
}

int32_t AdmProxy::MinSpeakerVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MinSpeakerVolume(minVolume);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetMicrophoneVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneVolume(volume);
  }
  return -1;
}

int32_t AdmProxy::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MaxMicrophoneVolume(maxVolume);
  }
  return -1;
}

int32_t AdmProxy::MinMicrophoneVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MinMicrophoneVolume(minVolume);
  }
  return -1;
}

int32_t AdmProxy::SpeakerMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetSpeakerMute(enable);
  }
  return -1;
}

int32_t AdmProxy::SpeakerMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SpeakerMute(enabled);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetMicrophoneMute(enable);
  }
  return -1;
}

int32_t AdmProxy::MicrophoneMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->MicrophoneMute(enabled);
  }
  return -1;
}

int32_t AdmProxy::StereoPlayoutIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoPlayoutIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::SetStereoPlayout(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetStereoPlayout(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoPlayout(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoPlayout(enabled);
  }
  *enabled = true;
  return 0;
}

int32_t AdmProxy::StereoRecordingIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoRecordingIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetStereoRecording(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetStereoRecording(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoRecording(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->StereoRecording(enabled);
  }
  *enabled = false;
  return 0;
}

int32_t AdmProxy::PlayoutDelay(uint16_t* delayMS) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->PlayoutDelay(delayMS);
  }
  *delayMS = 0;
  return 0;
}

bool AdmProxy::BuiltInAECIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInAECIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInAGCIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInAGCIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInNSIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->BuiltInNSIsAvailable();
  }
  return false;
}

int32_t AdmProxy::EnableBuiltInAEC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInAEC(enable);
  }
  return -1;
}

int32_t AdmProxy::EnableBuiltInAGC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInAGC(enable);
  }
  return -1;
}

int32_t AdmProxy::EnableBuiltInNS(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->EnableBuiltInNS(enable);
  }
  return -1;
}

#if defined(WEBRTC_IOS)
int AdmProxy::GetPlayoutAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->GetPlayoutAudioParameters(params);
  }
  return -1;
}

int AdmProxy::GetRecordAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->GetRecordAudioParameters(params);
  }
  return -1;
}
#endif

int32_t AdmProxy::SetObserver(webrtc::AudioDeviceObserver* observer) {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_) {
    return platform_adm_->SetObserver(observer);
  }
  return 0;
}

}  // namespace livekit_ffi
