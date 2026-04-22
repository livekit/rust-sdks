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

#include "livekit/adm_proxy.h"

#include "rtc_base/logging.h"
#include "rtc_base/thread.h"

namespace {
constexpr int kSampleRate = 48000;
constexpr int kChannels = 2;
constexpr int kBytesPerSample = kChannels * sizeof(int16_t);
constexpr int kSamplesPer10Ms = kSampleRate / 100;
}  // namespace

namespace livekit_ffi {

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env),
      worker_thread_(worker_thread),
      stub_data_(kSamplesPer10Ms * kChannels) {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::AdmProxy()";
}

AdmProxy::~AdmProxy() {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::~AdmProxy()";
  Terminate();
}

// Delegate swap implementation using snapshot pattern to avoid deadlocks.
// Pattern: lock → snapshot state → unlock → perform operations → reconcile
void AdmProxy::SetPlatformAdm(
    webrtc::scoped_refptr<webrtc::AudioDeviceModule> adm) {
  RTC_LOG(LS_INFO) << "AdmProxy::SetPlatformAdm()";

  // Step 1: Snapshot current state under lock
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> old_platform_adm;
  DelegateType old_type;
  bool was_initialized;
  bool was_playing;
  bool was_recording;
  bool was_playout_initialized;
  bool was_recording_initialized;
  webrtc::AudioTransport* transport;

  {
    webrtc::MutexLock lock(&mutex_);
    old_platform_adm = platform_adm_;
    old_type = delegate_type_;
    was_initialized = initialized_;
    was_playing = playing_;
    was_recording = recording_;
    was_playout_initialized = playout_initialized_;
    was_recording_initialized = recording_initialized_;
    transport = audio_transport_;

    // Update pointers atomically
    platform_adm_ = adm;
    delegate_type_ = adm ? DelegateType::kPlatform : DelegateType::kSynthetic;
  }

  // Step 2: Teardown old delegate OUTSIDE the lock
  // This avoids deadlock if delegate calls back into us
  if (old_type == DelegateType::kPlatform && old_platform_adm) {
    if (was_recording) old_platform_adm->StopRecording();
    if (was_playing) old_platform_adm->StopPlayout();
    old_platform_adm->RegisterAudioCallback(nullptr);
    old_platform_adm->Terminate();
  } else if (old_type == DelegateType::kSynthetic) {
    StopStubPlayoutTask();
  }

  // Step 3: Initialize new delegate OUTSIDE the lock
  if (adm && was_initialized) {
    adm->Init();
    adm->RegisterAudioCallback(transport);
    if (was_playout_initialized) {
      adm->InitPlayout();
    }
    if (was_recording_initialized) {
      adm->InitRecording();
    }
    if (was_playing) {
      adm->StartPlayout();
    }
    if (was_recording) {
      adm->StartRecording();
    }
  } else if (!adm && was_initialized && was_playing) {
    // Switching to synthetic mode while playing
    StartStubPlayoutTask();
  }
}

void AdmProxy::ClearDelegate() {
  RTC_LOG(LS_INFO) << "AdmProxy::ClearDelegate()";
  SetPlatformAdm(nullptr);
}

AdmProxy::DelegateType AdmProxy::delegate_type() const {
  webrtc::MutexLock lock(&mutex_);
  return delegate_type_;
}

bool AdmProxy::has_delegate() const {
  webrtc::MutexLock lock(&mutex_);
  return delegate_type_ != DelegateType::kSynthetic;
}

webrtc::scoped_refptr<webrtc::AudioDeviceModule> AdmProxy::platform_adm()
    const {
  webrtc::MutexLock lock(&mutex_);
  return platform_adm_;
}

webrtc::scoped_refptr<webrtc::AudioDeviceModule>
AdmProxy::GetPlatformAdmLocked() const {
  return platform_adm_;
}

void AdmProxy::StartStubPlayoutTask() {
  // Note: This creates a task that periodically pulls audio to keep
  // WebRTC's audio pipeline alive. This is NOT equivalent to real playout -
  // remote audio is discarded, AEC has no valid reference, and timing
  // may diverge from real audio hardware.
  //
  // This synthetic playout is only suitable for:
  // - Send-only scenarios with manual capture (NativeAudioSource)
  // - Testing/development without audio hardware
  //
  // It is NOT suitable for:
  // - Echo-cancelled bidirectional audio
  // - Real speaker playback
  if (stub_audio_queue_) {
    return;  // Already running
  }

  stub_audio_queue_ = env_.task_queue_factory().CreateTaskQueue(
      "AdmProxyStub", webrtc::TaskQueueFactory::Priority::NORMAL);

  // Capture transport pointer for use in task (avoid holding mutex in task)
  webrtc::AudioTransport* transport = nullptr;
  {
    webrtc::MutexLock lock(&mutex_);
    transport = audio_transport_;
  }

  stub_audio_task_ =
      webrtc::RepeatingTaskHandle::Start(stub_audio_queue_.get(), [this]() {
        // Quick check without lock - may race but that's acceptable
        // for this best-effort synthetic playout
        webrtc::AudioTransport* transport = nullptr;
        bool should_run = false;
        {
          webrtc::MutexLock lock(&mutex_);
          should_run = playing_ && delegate_type_ == DelegateType::kSynthetic;
          transport = audio_transport_;
        }

        if (should_run && transport) {
          int64_t elapsed_time_ms = -1;
          int64_t ntp_time_ms = -1;
          size_t n_samples_out = 0;
          void* data = stub_data_.data();

          // Pull audio data to keep WebRTC pipeline running
          // Note: This audio is discarded - not sent to any real device
          transport->NeedMorePlayData(kSamplesPer10Ms, kBytesPerSample,
                                      kChannels, kSampleRate, data,
                                      n_samples_out, &elapsed_time_ms,
                                      &ntp_time_ms);
        }

        return webrtc::TimeDelta::Millis(10);
      });
}

void AdmProxy::StopStubPlayoutTask() {
  stub_audio_queue_ = nullptr;  // Stops the task
}

// AudioDeviceModule interface implementation

int32_t AdmProxy::ActiveAudioLayer(AudioLayer* audioLayer) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->ActiveAudioLayer(audioLayer);
  }
  *audioLayer = AudioLayer::kDummyAudio;
  return 0;
}

int32_t AdmProxy::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  webrtc::MutexLock lock(&mutex_);
  audio_transport_ = transport;

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->RegisterAudioCallback(transport);
  }
  return 0;
}

int32_t AdmProxy::Init() {
  webrtc::MutexLock lock(&mutex_);
  if (initialized_) return 0;

  initialized_ = true;

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->Init();
  }
  return 0;
}

int32_t AdmProxy::Terminate() {
  // Snapshot and clear state under lock
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm;
  DelegateType type;
  bool was_recording;
  bool was_playing;

  {
    webrtc::MutexLock lock(&mutex_);
    if (!initialized_) return 0;

    platform_adm = platform_adm_;
    type = delegate_type_;
    was_recording = recording_;
    was_playing = playing_;

    initialized_ = false;
    playing_ = false;
    recording_ = false;
    playout_initialized_ = false;
    recording_initialized_ = false;
  }

  // Perform operations outside lock
  StopStubPlayoutTask();

  // IMPORTANT: Must stop recording/playout before Terminate() to properly
  // dispose hardware resources (e.g., VPIO AudioUnit on iOS).
  // See: https://github.com/aspect/issue - VPIO not disposed bug
  if (type == DelegateType::kPlatform && platform_adm) {
    if (was_recording) {
      RTC_LOG(LS_INFO) << "AdmProxy::Terminate() stopping recording";
      platform_adm->StopRecording();
    }
    if (was_playing) {
      RTC_LOG(LS_INFO) << "AdmProxy::Terminate() stopping playout";
      platform_adm->StopPlayout();
    }
    platform_adm->RegisterAudioCallback(nullptr);
    platform_adm->Terminate();
  }

  return 0;
}

bool AdmProxy::Initialized() const {
  webrtc::MutexLock lock(&mutex_);
  return initialized_;
}

int16_t AdmProxy::PlayoutDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->PlayoutDevices();
  }
  return 0;
}

int16_t AdmProxy::RecordingDevices() {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->RecordingDevices();
  }
  return 0;
}

int32_t AdmProxy::PlayoutDeviceName(uint16_t index,
                                    char name[webrtc::kAdmMaxDeviceNameSize],
                                    char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->PlayoutDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::RecordingDeviceName(uint16_t index,
                                      char name[webrtc::kAdmMaxDeviceNameSize],
                                      char guid[webrtc::kAdmMaxGuidSize]) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->RecordingDeviceName(index, name, guid);
  }
  return -1;
}

int32_t AdmProxy::SetPlayoutDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetPlayoutDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetPlayoutDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetPlayoutDevice(device);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(uint16_t index) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetRecordingDevice(index);
  }
  return 0;
}

int32_t AdmProxy::SetRecordingDevice(WindowsDeviceType device) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetRecordingDevice(device);
  }
  return 0;
}

int32_t AdmProxy::PlayoutIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->PlayoutIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::InitPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playout_initialized_ = true;

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->InitPlayout();
  }
  return 0;
}

bool AdmProxy::PlayoutIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->PlayoutIsInitialized();
  }
  return playout_initialized_;
}

int32_t AdmProxy::RecordingIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->RecordingIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::InitRecording() {
  webrtc::MutexLock lock(&mutex_);
  recording_initialized_ = true;

  RTC_LOG(LS_INFO) << "AdmProxy::InitRecording() delegate_type="
                   << static_cast<int>(delegate_type_);

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    int32_t result = platform_adm_->InitRecording();
    RTC_LOG(LS_INFO) << "Platform ADM InitRecording() returned: " << result;
    return result;
  }
  return 0;
}

bool AdmProxy::RecordingIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->RecordingIsInitialized();
  }
  return recording_initialized_;
}

int32_t AdmProxy::StartPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = true;

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StartPlayout();
  }

  // Synthetic mode - start pulling audio to keep pipeline running
  // Note: Audio is discarded, not played to any device
  StartStubPlayoutTask();
  return 0;
}

int32_t AdmProxy::StopPlayout() {
  webrtc::MutexLock lock(&mutex_);
  playing_ = false;

  StopStubPlayoutTask();

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StopPlayout();
  }
  return 0;
}

bool AdmProxy::Playing() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->Playing();
  }
  return playing_;
}

int32_t AdmProxy::StartRecording() {
  webrtc::MutexLock lock(&mutex_);
  recording_ = true;

  RTC_LOG(LS_INFO) << "AdmProxy::StartRecording() delegate_type="
                   << static_cast<int>(delegate_type_);

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    int32_t result = platform_adm_->StartRecording();
    RTC_LOG(LS_INFO) << "Platform ADM StartRecording() returned: " << result;
    return result;
  }
  RTC_LOG(LS_WARNING) << "StartRecording() called but no ADM delegate set!";
  return 0;
}

int32_t AdmProxy::StopRecording() {
  webrtc::MutexLock lock(&mutex_);
  recording_ = false;

  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StopRecording();
  }
  return 0;
}

bool AdmProxy::Recording() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->Recording();
  }
  return recording_;
}

int32_t AdmProxy::InitSpeaker() {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->InitSpeaker();
  }
  return 0;
}

bool AdmProxy::SpeakerIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SpeakerIsInitialized();
  }
  return false;
}

int32_t AdmProxy::InitMicrophone() {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->InitMicrophone();
  }
  return 0;
}

bool AdmProxy::MicrophoneIsInitialized() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MicrophoneIsInitialized();
  }
  return false;
}

int32_t AdmProxy::SpeakerVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SpeakerVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetSpeakerVolume(volume);
  }
  return 0;
}

int32_t AdmProxy::SpeakerVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SpeakerVolume(volume);
  }
  return 0;
}

int32_t AdmProxy::MaxSpeakerVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MaxSpeakerVolume(maxVolume);
  }
  return 0;
}

int32_t AdmProxy::MinSpeakerVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MinSpeakerVolume(minVolume);
  }
  return 0;
}

int32_t AdmProxy::MicrophoneVolumeIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MicrophoneVolumeIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneVolume(uint32_t volume) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetMicrophoneVolume(volume);
  }
  return 0;
}

int32_t AdmProxy::MicrophoneVolume(uint32_t* volume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MicrophoneVolume(volume);
  }
  return 0;
}

int32_t AdmProxy::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MaxMicrophoneVolume(maxVolume);
  }
  return 0;
}

int32_t AdmProxy::MinMicrophoneVolume(uint32_t* minVolume) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MinMicrophoneVolume(minVolume);
  }
  return 0;
}

int32_t AdmProxy::SpeakerMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SpeakerMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetSpeakerMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetSpeakerMute(enable);
  }
  return 0;
}

int32_t AdmProxy::SpeakerMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SpeakerMute(enabled);
  }
  return 0;
}

int32_t AdmProxy::MicrophoneMuteIsAvailable(bool* available) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MicrophoneMuteIsAvailable(available);
  }
  *available = false;
  return 0;
}

int32_t AdmProxy::SetMicrophoneMute(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetMicrophoneMute(enable);
  }
  return 0;
}

int32_t AdmProxy::MicrophoneMute(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->MicrophoneMute(enabled);
  }
  return 0;
}

int32_t AdmProxy::StereoPlayoutIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StereoPlayoutIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::SetStereoPlayout(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetStereoPlayout(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoPlayout(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StereoPlayout(enabled);
  }
  *enabled = true;
  return 0;
}

int32_t AdmProxy::StereoRecordingIsAvailable(bool* available) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StereoRecordingIsAvailable(available);
  }
  *available = true;
  return 0;
}

int32_t AdmProxy::SetStereoRecording(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetStereoRecording(enable);
  }
  return 0;
}

int32_t AdmProxy::StereoRecording(bool* enabled) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->StereoRecording(enabled);
  }
  *enabled = true;
  return 0;
}

int32_t AdmProxy::PlayoutDelay(uint16_t* delayMS) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->PlayoutDelay(delayMS);
  }
  *delayMS = 0;
  return 0;
}

bool AdmProxy::BuiltInAECIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->BuiltInAECIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInAGCIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->BuiltInAGCIsAvailable();
  }
  return false;
}

bool AdmProxy::BuiltInNSIsAvailable() const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->BuiltInNSIsAvailable();
  }
  return false;
}

int32_t AdmProxy::EnableBuiltInAEC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->EnableBuiltInAEC(enable);
  }
  return 0;
}

int32_t AdmProxy::EnableBuiltInAGC(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->EnableBuiltInAGC(enable);
  }
  return 0;
}

int32_t AdmProxy::EnableBuiltInNS(bool enable) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->EnableBuiltInNS(enable);
  }
  return 0;
}

#if defined(WEBRTC_IOS)
int AdmProxy::GetPlayoutAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->GetPlayoutAudioParameters(params);
  }
  return 0;
}

int AdmProxy::GetRecordAudioParameters(webrtc::AudioParameters* params) const {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->GetRecordAudioParameters(params);
  }
  return 0;
}
#endif

int32_t AdmProxy::SetObserver(webrtc::AudioDeviceObserver* observer) {
  webrtc::MutexLock lock(&mutex_);
  if (delegate_type_ == DelegateType::kPlatform && platform_adm_) {
    return platform_adm_->SetObserver(observer);
  }
  return 0;
}

}  // namespace livekit_ffi
