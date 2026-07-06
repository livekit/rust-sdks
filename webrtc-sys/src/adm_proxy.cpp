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
#include "api/make_ref_counted.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"

#if defined(__ANDROID__)
#include <jni.h>
#include "sdk/android/native_api/audio_device_module/audio_device_android.h"
#include "sdk/android/native_api/base/init.h"
#endif

namespace livekit_ffi {

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env),
      worker_thread_(worker_thread) {
  RTC_DCHECK(worker_thread_);
  // The proxy must be constructed on the worker thread so the platform ADM
  // created below binds its sequence checker to it.
  RTC_DCHECK_RUN_ON(worker_thread_);

  // Create the synthetic ADM for synthetic mode. SyntheticAudioDevice pumps
  // the WebRTC audio pipeline without platform audio, allowing FFI callbacks
  // to receive decoded remote audio.
  synthetic_adm_ = webrtc::make_ref_counted<SyntheticAudioDevice>(env_);
  if (synthetic_adm_->Init() != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Failed to initialize synthetic ADM";
  }

  // Create the Platform ADM for real audio I/O.
  // This is created immediately (not lazily) for iOS compatibility.
  // iOS audio session requires early setup to avoid KVO race conditions.
  // On Android, we defer Platform ADM creation to AcquirePlatformAdm().
  // This is because:
  // 1. CreateAudioDeviceModule requires JNI to be fully initialized
  // 2. The JNI initialization (via JNI_OnLoad or manual init) may not have
  //    completed by the time the AdmProxy constructor runs
  // 3. Deferring creation ensures JNI is ready when we actually need the ADM
#if defined(__ANDROID__)
  // platform_adm_ stays nullptr, will be created in EnsurePlatformAdmCreated()
#else
#if defined(WEBRTC_IOS) || defined(WEBRTC_MAC)
  // Use the AVAudioEngine based ADM on Apple platforms. It supports runtime
  // switchable voice processing and device change handling.
  platform_adm_ = webrtc::CreateAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kAppleAudioEngine);
#else
  platform_adm_ = webrtc::CreateAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);
#endif

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: CreateAudioDeviceModule returned nullptr";
  } else {
    int32_t init_result = platform_adm_->Init();
    if (init_result != 0) {
      RTC_LOG(LS_ERROR) << "AdmProxy: Platform ADM Init() failed with error=" << init_result;
      platform_adm_ = nullptr;
    }
  }
#endif
}

AdmProxy::~AdmProxy() {
  RTC_LOG(LS_VERBOSE) << "AdmProxy::~AdmProxy()";

  // The last reference may be dropped on any thread. Tear down on the worker
  // so the sub ADMs are terminated and destroyed on their owning thread.
  RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (synthetic_adm_) {
      synthetic_adm_->Terminate();
      synthetic_adm_ = nullptr;
    }
    if (platform_adm_) {
      platform_adm_->Terminate();
      platform_adm_ = nullptr;
    }
  });
}

// =============================================================================
// Helper Methods (worker thread only)
// =============================================================================

bool AdmProxy::IsPlatformPlayoutActive() const {
  // Platform playout is active when: ref_count > 0 AND playout explicitly enabled.
  // Otherwise, synthetic mode handles playout via the internal pumping task.
  return platform_adm_ && platform_adm_ref_count_ > 0 && playout_enabled_;
}

webrtc::AudioDeviceModule* AdmProxy::RecordingAdm() const {
  // Recording only available through platform ADM when enabled.
  // Synthetic mode doesn't support recording (no microphone).
  if (platform_adm_ && platform_adm_ref_count_ > 0 && recording_enabled_) {
    return platform_adm_.get();
  }
  return nullptr;
}

// =============================================================================
// Platform ADM Lifecycle Management
// =============================================================================

#if defined(__ANDROID__)
bool AdmProxy::EnsurePlatformAdmCreated() {
  if (platform_adm_) {
    return true;  // Already created
  }

  // Use CreateAndroidAudioDeviceModule which properly uses GetAppContext()
  // to get the application context set via ContextUtils.initialize().
  platform_adm_ = webrtc::CreateAndroidAudioDeviceModule(
      env_, webrtc::AudioDeviceModule::kPlatformDefaultAudio);

  if (!platform_adm_) {
    RTC_LOG(LS_ERROR) << "AdmProxy: CreateAndroidAudioDeviceModule returned nullptr. "
                      << "Ensure ContextUtils.initialize() was called.";
    return false;
  }

  int32_t init_result = platform_adm_->Init();
  if (init_result != 0) {
    RTC_LOG(LS_ERROR) << "AdmProxy: Platform ADM Init() failed with error=" << init_result;
    platform_adm_ = nullptr;
    return false;
  }

  // WebRTC registered its audio transport before the ADM existed,
  // pass it along so recorded audio actually reaches the pipeline
  if (audio_transport_) {
    platform_adm_->RegisterAudioCallback(audio_transport_);
  }

  // Re-apply any device selection made before the ADM existed
  if (selected_playout_device_ != 0) {
    platform_adm_->SetPlayoutDevice(selected_playout_device_);
  }
  if (selected_recording_device_ != 0) {
    platform_adm_->SetRecordingDevice(selected_recording_device_);
  }

  return true;
}
#endif

bool AdmProxy::AcquirePlatformAdm() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);

#if defined(__ANDROID__)
    // On Android, lazily create the Platform ADM on first acquire.
    // This ensures JNI is fully initialized before we try to create the ADM.
    if (!EnsurePlatformAdmCreated()) {
      RTC_LOG(LS_ERROR) << "AdmProxy::AcquirePlatformAdm() - Failed to create Platform ADM";
      return false;
    }
#else
    if (!platform_adm_) {
      RTC_LOG(LS_ERROR) << "AdmProxy::AcquirePlatformAdm() - Platform ADM not available";
      return false;
    }
#endif

    int old_ref_count = platform_adm_ref_count_;
    platform_adm_ref_count_++;

    // If this is the first acquisition and playout/recording is enabled,
    // we may need to switch from synthetic mode to platform ADM
    if (old_ref_count == 0) {
      SwitchPlayoutMode();
      SwitchRecordingAdm();
    }

    return true;
  });
}

void AdmProxy::ReleasePlatformAdm() {
  RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);

    if (platform_adm_ref_count_ <= 0) {
      RTC_LOG(LS_WARNING) << "AdmProxy::ReleasePlatformAdm() called with ref_count="
                          << platform_adm_ref_count_;
      return;
    }

    platform_adm_ref_count_--;

    // If ref_count reaches 0, switch back from platform ADM to synthetic mode
    // Note: We do NOT terminate the Platform ADM - it stays alive until destructor.
    // This avoids iOS KVO race conditions from re-creating the ADM.
    if (platform_adm_ref_count_ == 0) {
      SwitchPlayoutMode();
      SwitchRecordingAdm();
    }
  });
}

int AdmProxy::platform_adm_ref_count() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    return platform_adm_ref_count_;
  });
}

bool AdmProxy::is_platform_adm_active() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    return platform_adm_ != nullptr && platform_adm_ref_count_ > 0;
  });
}

// =============================================================================
// Recording/Playout Control
// =============================================================================

void AdmProxy::set_recording_enabled(bool enabled) {
  RunOnWorker([this, enabled] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (recording_enabled_ == enabled) {
      return;
    }
    recording_enabled_ = enabled;
    SwitchRecordingAdm();
  });
}

bool AdmProxy::recording_enabled() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    return recording_enabled_;
  });
}

void AdmProxy::set_playout_enabled(bool enabled) {
  RunOnWorker([this, enabled] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (playout_enabled_ == enabled) {
      return;
    }
    playout_enabled_ = enabled;
    SwitchPlayoutMode();
  });
}

bool AdmProxy::playout_enabled() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    return playout_enabled_;
  });
}

// =============================================================================
// Mode Switching Helpers (worker thread only)
// =============================================================================

void AdmProxy::SwitchPlayoutMode() {
  if (!playing_) return;

  if (IsPlatformPlayoutActive()) {
    // Switch to platform mode - stop synthetic, start platform ADM
    if (synthetic_adm_) {
      synthetic_adm_->StopPlayout();
    }
    if (platform_adm_) {
      platform_adm_->InitPlayout();
      platform_adm_->StartPlayout();
    }
  } else {
    // Switch to synthetic mode - stop platform ADM, start synthetic ADM
    if (platform_adm_) {
      platform_adm_->StopPlayout();
    }
    if (synthetic_adm_) {
      synthetic_adm_->StartPlayout();
    }
  }
}

void AdmProxy::SwitchRecordingAdm() {
  if (!recording_) return;

  // Stop platform ADM recording (only one that supports recording)
  if (platform_adm_) platform_adm_->StopRecording();

  // Start if new ADM supports recording
  auto* adm = RecordingAdm();
  if (adm) {
    adm->InitRecording();
    adm->StartRecording();
  } else {
    recording_ = false;
  }
}

// =============================================================================
// AudioDeviceModule Interface Implementation
// =============================================================================

int32_t AdmProxy::ActiveAudioLayer(AudioLayer* audioLayer) const {
  return RunOnWorker([this, audioLayer] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->ActiveAudioLayer(audioLayer);
    }
    *audioLayer = AudioLayer::kDummyAudio;
    return 0;
  });
}

int32_t AdmProxy::RegisterAudioCallback(webrtc::AudioTransport* transport) {
  return RunOnWorker([this, transport] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    audio_transport_ = transport;

    // Register with both ADMs so they're ready when we switch modes
    if (synthetic_adm_) {
      synthetic_adm_->RegisterAudioCallback(transport);
    }
    if (platform_adm_) {
      platform_adm_->RegisterAudioCallback(transport);
    }
    return 0;
  });
}

int32_t AdmProxy::Init() {
  // Init is a no-op - the sub ADMs are initialized at creation time
  return 0;
}

int32_t AdmProxy::Terminate() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    int32_t result = 0;
    if (synthetic_adm_) {
      result = synthetic_adm_->Terminate();
    }
    if (platform_adm_) {
      int32_t platform_result = platform_adm_->Terminate();
      if (result == 0) result = platform_result;
    }
    return result;
  });
}

bool AdmProxy::Initialized() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    // We're initialized if at least one ADM is initialized
    bool synthetic_init = synthetic_adm_ && synthetic_adm_->Initialized();
    bool platform_init = platform_adm_ && platform_adm_->Initialized();
    return synthetic_init || platform_init;
  });
}

int16_t AdmProxy::PlayoutDevices() {
  // In synthetic mode, there are no platform devices
  return WithPlatformAdm<int16_t>(0, [](webrtc::AudioDeviceModule& adm) {
    return adm.PlayoutDevices();
  });
}

int16_t AdmProxy::RecordingDevices() {
  // In synthetic mode, there are no platform devices
  return WithPlatformAdm<int16_t>(0, [](webrtc::AudioDeviceModule& adm) {
    return adm.RecordingDevices();
  });
}

int32_t AdmProxy::PlayoutDeviceName(uint16_t index,
                                    char name[webrtc::kAdmMaxDeviceNameSize],
                                    char guid[webrtc::kAdmMaxGuidSize]) {
  return WithPlatformAdm<int32_t>(-1, [&](webrtc::AudioDeviceModule& adm) {
    return adm.PlayoutDeviceName(index, name, guid);
  });
}

int32_t AdmProxy::RecordingDeviceName(uint16_t index,
                                      char name[webrtc::kAdmMaxDeviceNameSize],
                                      char guid[webrtc::kAdmMaxGuidSize]) {
  return WithPlatformAdm<int32_t>(-1, [&](webrtc::AudioDeviceModule& adm) {
    return adm.RecordingDeviceName(index, name, guid);
  });
}

int32_t AdmProxy::SetPlayoutDevice(uint16_t index) {
  return RunOnWorker([this, index] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    selected_playout_device_ = index;
    if (platform_adm_) {
      return platform_adm_->SetPlayoutDevice(index);
    }
    return 0;
  });
}

int32_t AdmProxy::SetPlayoutDevice(WindowsDeviceType device) {
  return RunOnWorker([this, device] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->SetPlayoutDevice(device);
    }
    return 0;
  });
}

int32_t AdmProxy::SetRecordingDevice(uint16_t index) {
  return RunOnWorker([this, index] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    selected_recording_device_ = index;
    if (platform_adm_) {
      return platform_adm_->SetRecordingDevice(index);
    }
    return 0;
  });
}

int32_t AdmProxy::SetRecordingDevice(WindowsDeviceType device) {
  return RunOnWorker([this, device] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->SetRecordingDevice(device);
    }
    return 0;
  });
}

int32_t AdmProxy::PlayoutIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->PlayoutIsAvailable(available);
    }
    *available = true;  // Synthetic playout is always available
    return 0;
  });
}

int32_t AdmProxy::InitPlayout() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (IsPlatformPlayoutActive()) {
      return platform_adm_->InitPlayout();
    }
    // Synthetic mode
    if (synthetic_adm_) {
      return synthetic_adm_->InitPlayout();
    }
    return -1;
  });
}

bool AdmProxy::PlayoutIsInitialized() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (IsPlatformPlayoutActive()) {
      return platform_adm_->PlayoutIsInitialized();
    }
    // Synthetic mode
    return synthetic_adm_ && synthetic_adm_->PlayoutIsInitialized();
  });
}

int32_t AdmProxy::RecordingIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->RecordingIsAvailable(available);
    }
    *available = false;  // Recording not available in synthetic mode
    return 0;
  });
}

int32_t AdmProxy::InitRecording() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    auto* adm = RecordingAdm();
    if (!adm) {
      // Recording not available (no platform ADM or recording disabled)
      // Return success to avoid breaking WebRTC's initialization flow
      return 0;
    }
    return adm->InitRecording();
  });
}

bool AdmProxy::RecordingIsInitialized() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    auto* adm = RecordingAdm();
    if (adm) {
      return adm->RecordingIsInitialized();
    }
    return false;  // Recording not available
  });
}

int32_t AdmProxy::StartPlayout() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    playing_ = true;

    if (IsPlatformPlayoutActive()) {
      return platform_adm_->StartPlayout();
    }
    // Synthetic mode
    if (synthetic_adm_) {
      return synthetic_adm_->StartPlayout();
    }
    return -1;
  });
}

int32_t AdmProxy::StopPlayout() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    playing_ = false;

    // Stop both ADMs
    if (synthetic_adm_) {
      synthetic_adm_->StopPlayout();
    }
    if (platform_adm_) {
      platform_adm_->StopPlayout();
    }
    return 0;
  });
}

bool AdmProxy::Playing() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (IsPlatformPlayoutActive()) {
      return platform_adm_->Playing();
    }
    return synthetic_adm_ && synthetic_adm_->Playing();
  });
}

int32_t AdmProxy::StartRecording() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    auto* adm = RecordingAdm();
    if (!adm) {
      // Recording not available - return success to avoid breaking WebRTC
      return 0;
    }
    recording_ = true;
    return adm->StartRecording();
  });
}

int32_t AdmProxy::StopRecording() {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    recording_ = false;

    auto* adm = RecordingAdm();
    if (adm) {
      return adm->StopRecording();
    }
    return 0;
  });
}

bool AdmProxy::Recording() const {
  return RunOnWorker([this] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    auto* adm = RecordingAdm();
    if (adm) {
      return adm->Recording();
    }
    return false;
  });
}

int32_t AdmProxy::InitSpeaker() {
  return WithPlatformAdm<int32_t>(0, [](webrtc::AudioDeviceModule& adm) {
    return adm.InitSpeaker();
  });
}

bool AdmProxy::SpeakerIsInitialized() const {
  return WithPlatformAdm<bool>(true, [](webrtc::AudioDeviceModule& adm) {
    return adm.SpeakerIsInitialized();
  });
}

int32_t AdmProxy::InitMicrophone() {
  return WithPlatformAdm<int32_t>(0, [](webrtc::AudioDeviceModule& adm) {
    return adm.InitMicrophone();
  });
}

bool AdmProxy::MicrophoneIsInitialized() const {
  return WithPlatformAdm<bool>(false, [](webrtc::AudioDeviceModule& adm) {
    return adm.MicrophoneIsInitialized();
  });
}

int32_t AdmProxy::SpeakerVolumeIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->SpeakerVolumeIsAvailable(available);
    }
    *available = false;
    return 0;
  });
}

int32_t AdmProxy::SetSpeakerVolume(uint32_t volume) {
  return WithPlatformAdm<int32_t>(-1, [volume](webrtc::AudioDeviceModule& adm) {
    return adm.SetSpeakerVolume(volume);
  });
}

int32_t AdmProxy::SpeakerVolume(uint32_t* volume) const {
  return WithPlatformAdm<int32_t>(-1, [volume](webrtc::AudioDeviceModule& adm) {
    return adm.SpeakerVolume(volume);
  });
}

int32_t AdmProxy::MaxSpeakerVolume(uint32_t* maxVolume) const {
  return WithPlatformAdm<int32_t>(-1, [maxVolume](webrtc::AudioDeviceModule& adm) {
    return adm.MaxSpeakerVolume(maxVolume);
  });
}

int32_t AdmProxy::MinSpeakerVolume(uint32_t* minVolume) const {
  return WithPlatformAdm<int32_t>(-1, [minVolume](webrtc::AudioDeviceModule& adm) {
    return adm.MinSpeakerVolume(minVolume);
  });
}

int32_t AdmProxy::MicrophoneVolumeIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->MicrophoneVolumeIsAvailable(available);
    }
    *available = false;
    return 0;
  });
}

int32_t AdmProxy::SetMicrophoneVolume(uint32_t volume) {
  return WithPlatformAdm<int32_t>(-1, [volume](webrtc::AudioDeviceModule& adm) {
    return adm.SetMicrophoneVolume(volume);
  });
}

int32_t AdmProxy::MicrophoneVolume(uint32_t* volume) const {
  return WithPlatformAdm<int32_t>(-1, [volume](webrtc::AudioDeviceModule& adm) {
    return adm.MicrophoneVolume(volume);
  });
}

int32_t AdmProxy::MaxMicrophoneVolume(uint32_t* maxVolume) const {
  return WithPlatformAdm<int32_t>(-1, [maxVolume](webrtc::AudioDeviceModule& adm) {
    return adm.MaxMicrophoneVolume(maxVolume);
  });
}

int32_t AdmProxy::MinMicrophoneVolume(uint32_t* minVolume) const {
  return WithPlatformAdm<int32_t>(-1, [minVolume](webrtc::AudioDeviceModule& adm) {
    return adm.MinMicrophoneVolume(minVolume);
  });
}

int32_t AdmProxy::SpeakerMuteIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->SpeakerMuteIsAvailable(available);
    }
    *available = false;
    return 0;
  });
}

int32_t AdmProxy::SetSpeakerMute(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.SetSpeakerMute(enable);
  });
}

int32_t AdmProxy::SpeakerMute(bool* enabled) const {
  return WithPlatformAdm<int32_t>(-1, [enabled](webrtc::AudioDeviceModule& adm) {
    return adm.SpeakerMute(enabled);
  });
}

int32_t AdmProxy::MicrophoneMuteIsAvailable(bool* available) {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->MicrophoneMuteIsAvailable(available);
    }
    *available = false;
    return 0;
  });
}

int32_t AdmProxy::SetMicrophoneMute(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.SetMicrophoneMute(enable);
  });
}

int32_t AdmProxy::MicrophoneMute(bool* enabled) const {
  return WithPlatformAdm<int32_t>(-1, [enabled](webrtc::AudioDeviceModule& adm) {
    return adm.MicrophoneMute(enabled);
  });
}

int32_t AdmProxy::StereoPlayoutIsAvailable(bool* available) const {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->StereoPlayoutIsAvailable(available);
    }
    *available = true;
    return 0;
  });
}

int32_t AdmProxy::SetStereoPlayout(bool enable) {
  return WithPlatformAdm<int32_t>(0, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.SetStereoPlayout(enable);
  });
}

int32_t AdmProxy::StereoPlayout(bool* enabled) const {
  return RunOnWorker([this, enabled] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->StereoPlayout(enabled);
    }
    *enabled = true;
    return 0;
  });
}

int32_t AdmProxy::StereoRecordingIsAvailable(bool* available) const {
  return RunOnWorker([this, available] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->StereoRecordingIsAvailable(available);
    }
    *available = false;
    return 0;
  });
}

int32_t AdmProxy::SetStereoRecording(bool enable) {
  return WithPlatformAdm<int32_t>(0, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.SetStereoRecording(enable);
  });
}

int32_t AdmProxy::StereoRecording(bool* enabled) const {
  return RunOnWorker([this, enabled] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (platform_adm_) {
      return platform_adm_->StereoRecording(enabled);
    }
    *enabled = false;
    return 0;
  });
}

int32_t AdmProxy::PlayoutDelay(uint16_t* delayMS) const {
  return RunOnWorker([this, delayMS] {
    RTC_DCHECK_RUN_ON(worker_thread_);
    if (IsPlatformPlayoutActive()) {
      return platform_adm_->PlayoutDelay(delayMS);
    }
    if (synthetic_adm_) {
      return synthetic_adm_->PlayoutDelay(delayMS);
    }
    *delayMS = 0;
    return 0;
  });
}

bool AdmProxy::BuiltInAECIsAvailable() const {
  return WithPlatformAdm<bool>(false, [](webrtc::AudioDeviceModule& adm) {
    return adm.BuiltInAECIsAvailable();
  });
}

bool AdmProxy::BuiltInAGCIsAvailable() const {
  return WithPlatformAdm<bool>(false, [](webrtc::AudioDeviceModule& adm) {
    return adm.BuiltInAGCIsAvailable();
  });
}

bool AdmProxy::BuiltInNSIsAvailable() const {
  return WithPlatformAdm<bool>(false, [](webrtc::AudioDeviceModule& adm) {
    return adm.BuiltInNSIsAvailable();
  });
}

int32_t AdmProxy::EnableBuiltInAEC(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.EnableBuiltInAEC(enable);
  });
}

int32_t AdmProxy::EnableBuiltInAGC(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.EnableBuiltInAGC(enable);
  });
}

int32_t AdmProxy::EnableBuiltInNS(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.EnableBuiltInNS(enable);
  });
}

webrtc::AudioDeviceModule::PlatformAudioProcessingTopology
AdmProxy::GetPlatformAudioProcessingTopology() const {
  return WithPlatformAdm<
      webrtc::AudioDeviceModule::PlatformAudioProcessingTopology>(
      webrtc::AudioDeviceModule::PlatformAudioProcessingTopology::kIndependent,
      [](webrtc::AudioDeviceModule& adm) {
        return adm.GetPlatformAudioProcessingTopology();
      });
}

bool AdmProxy::PlatformVoiceProcessingPathIsAvailable() const {
  return WithPlatformAdm<bool>(false, [](webrtc::AudioDeviceModule& adm) {
    return adm.PlatformVoiceProcessingPathIsAvailable();
  });
}

int32_t AdmProxy::EnablePlatformVoiceProcessingPath(bool enable) {
  return WithPlatformAdm<int32_t>(-1, [enable](webrtc::AudioDeviceModule& adm) {
    return adm.EnablePlatformVoiceProcessingPath(enable);
  });
}

webrtc::AudioDeviceModule::PlatformAudioProcessingState
AdmProxy::GetPlatformAudioProcessingState() const {
  return WithPlatformAdm<
      webrtc::AudioDeviceModule::PlatformAudioProcessingState>(
      webrtc::AudioDeviceModule::PlatformAudioProcessingState(),
      [](webrtc::AudioDeviceModule& adm) {
        return adm.GetPlatformAudioProcessingState();
      });
}

#if defined(WEBRTC_IOS)
int AdmProxy::GetPlayoutAudioParameters(webrtc::AudioParameters* params) const {
  return WithPlatformAdm<int>(-1, [params](webrtc::AudioDeviceModule& adm) {
    return adm.GetPlayoutAudioParameters(params);
  });
}

int AdmProxy::GetRecordAudioParameters(webrtc::AudioParameters* params) const {
  return WithPlatformAdm<int>(-1, [params](webrtc::AudioDeviceModule& adm) {
    return adm.GetRecordAudioParameters(params);
  });
}
#endif

int32_t AdmProxy::SetObserver(webrtc::AudioDeviceObserver* observer) {
  return WithPlatformAdm<int32_t>(0, [observer](webrtc::AudioDeviceModule& adm) {
    return adm.SetObserver(observer);
  });
}

}  // namespace livekit_ffi
