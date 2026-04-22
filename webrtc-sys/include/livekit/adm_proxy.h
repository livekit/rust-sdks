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

#pragma once

#include <atomic>
#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/scoped_refptr.h"
#include "api/task_queue/task_queue_base.h"
#include "modules/audio_device/include/audio_device.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/task_utils/repeating_task.h"

namespace webrtc {
class Thread;
}  // namespace webrtc

namespace livekit_ffi {

// Forward declarations
class AdmProxy;

// ADM Proxy that can delegate to different implementations at runtime.
//
// Supports two modes:
// - Synthetic: Manual audio capture via NativeAudioSource, synthetic playout (default)
// - Platform: WebRTC's built-in platform-specific ADM (FFI only)
//
// Note: Custom ADM support has been removed. Platform ADM is only available
// via the FFI interface, not in the public Rust SDK.
//
// IMPORTANT: Delegate swapping is supported but has limitations:
// - Active capture/playout may be briefly interrupted during swap
// - AEC state may be affected when switching modes
// - Some transitions may require audio restart for full effect
// - Swap is "best effort" - not all state can be perfectly preserved
class AdmProxy : public webrtc::AudioDeviceModule {
 public:
  enum class DelegateType {
    kSynthetic,  // Synthetic ADM with manual capture (NativeAudioSource)
    kPlatform    // WebRTC's platform-specific ADM (FFI only)
  };

  explicit AdmProxy(const webrtc::Environment& env,
                    webrtc::Thread* worker_thread);
  ~AdmProxy() override;

  // Runtime delegate management - THREAD SAFE
  // These can be called from any thread at any time
  void SetPlatformAdm(webrtc::scoped_refptr<webrtc::AudioDeviceModule> adm);
  void ClearDelegate();  // Revert to stub behavior

  DelegateType delegate_type() const;
  bool has_delegate() const;

  // Access the underlying platform ADM (if set) for device enumeration
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm() const;

  // AudioDeviceModule interface implementation
  int32_t ActiveAudioLayer(AudioLayer* audioLayer) const override;
  int32_t RegisterAudioCallback(webrtc::AudioTransport* transport) override;

  int32_t Init() override;
  int32_t Terminate() override;
  bool Initialized() const override;

  int16_t PlayoutDevices() override;
  int16_t RecordingDevices() override;
  int32_t PlayoutDeviceName(uint16_t index,
                            char name[webrtc::kAdmMaxDeviceNameSize],
                            char guid[webrtc::kAdmMaxGuidSize]) override;
  int32_t RecordingDeviceName(uint16_t index,
                              char name[webrtc::kAdmMaxDeviceNameSize],
                              char guid[webrtc::kAdmMaxGuidSize]) override;

  int32_t SetPlayoutDevice(uint16_t index) override;
  int32_t SetPlayoutDevice(WindowsDeviceType device) override;
  int32_t SetRecordingDevice(uint16_t index) override;
  int32_t SetRecordingDevice(WindowsDeviceType device) override;

  int32_t PlayoutIsAvailable(bool* available) override;
  int32_t InitPlayout() override;
  bool PlayoutIsInitialized() const override;
  int32_t RecordingIsAvailable(bool* available) override;
  int32_t InitRecording() override;
  bool RecordingIsInitialized() const override;

  int32_t StartPlayout() override;
  int32_t StopPlayout() override;
  bool Playing() const override;
  int32_t StartRecording() override;
  int32_t StopRecording() override;
  bool Recording() const override;

  int32_t InitSpeaker() override;
  bool SpeakerIsInitialized() const override;
  int32_t InitMicrophone() override;
  bool MicrophoneIsInitialized() const override;

  int32_t SpeakerVolumeIsAvailable(bool* available) override;
  int32_t SetSpeakerVolume(uint32_t volume) override;
  int32_t SpeakerVolume(uint32_t* volume) const override;
  int32_t MaxSpeakerVolume(uint32_t* maxVolume) const override;
  int32_t MinSpeakerVolume(uint32_t* minVolume) const override;

  int32_t MicrophoneVolumeIsAvailable(bool* available) override;
  int32_t SetMicrophoneVolume(uint32_t volume) override;
  int32_t MicrophoneVolume(uint32_t* volume) const override;
  int32_t MaxMicrophoneVolume(uint32_t* maxVolume) const override;
  int32_t MinMicrophoneVolume(uint32_t* minVolume) const override;

  int32_t SpeakerMuteIsAvailable(bool* available) override;
  int32_t SetSpeakerMute(bool enable) override;
  int32_t SpeakerMute(bool* enabled) const override;

  int32_t MicrophoneMuteIsAvailable(bool* available) override;
  int32_t SetMicrophoneMute(bool enable) override;
  int32_t MicrophoneMute(bool* enabled) const override;

  int32_t StereoPlayoutIsAvailable(bool* available) const override;
  int32_t SetStereoPlayout(bool enable) override;
  int32_t StereoPlayout(bool* enabled) const override;
  int32_t StereoRecordingIsAvailable(bool* available) const override;
  int32_t SetStereoRecording(bool enable) override;
  int32_t StereoRecording(bool* enabled) const override;

  int32_t PlayoutDelay(uint16_t* delayMS) const override;

  bool BuiltInAECIsAvailable() const override;
  bool BuiltInAGCIsAvailable() const override;
  bool BuiltInNSIsAvailable() const override;

  int32_t EnableBuiltInAEC(bool enable) override;
  int32_t EnableBuiltInAGC(bool enable) override;
  int32_t EnableBuiltInNS(bool enable) override;

#if defined(WEBRTC_IOS)
  int GetPlayoutAudioParameters(webrtc::AudioParameters* params) const override;
  int GetRecordAudioParameters(webrtc::AudioParameters* params) const override;
#endif

  int32_t SetObserver(webrtc::AudioDeviceObserver* observer) override;

 private:
  // Stub implementation for when no delegate is set
  void StartStubPlayoutTask();
  void StopStubPlayoutTask();

  // Helper to safely get delegate under lock
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> GetPlatformAdmLocked() const;

  mutable webrtc::Mutex mutex_;

  // Delegate references (protected by mutex_)
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_
      RTC_GUARDED_BY(mutex_);
  DelegateType delegate_type_ RTC_GUARDED_BY(mutex_) = DelegateType::kSynthetic;

  // State tracking for delegate swaps (protected by mutex_)
  webrtc::AudioTransport* audio_transport_ RTC_GUARDED_BY(mutex_) = nullptr;
  bool initialized_ RTC_GUARDED_BY(mutex_) = false;
  bool playing_ RTC_GUARDED_BY(mutex_) = false;
  bool recording_ RTC_GUARDED_BY(mutex_) = false;
  bool playout_initialized_ RTC_GUARDED_BY(mutex_) = false;
  bool recording_initialized_ RTC_GUARDED_BY(mutex_) = false;

  // Stub playout task (for when no delegate is set)
  const webrtc::Environment& env_;
  webrtc::Thread* worker_thread_;
  std::vector<int16_t> stub_data_;
  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> stub_audio_queue_;
  webrtc::RepeatingTaskHandle stub_audio_task_;
};

}  // namespace livekit_ffi
