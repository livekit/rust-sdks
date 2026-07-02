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

#include <utility>

#include "api/environment/environment.h"
#include "api/scoped_refptr.h"
#include "api/sequence_checker.h"
#include "livekit/synthetic_audio_device.h"
#include "modules/audio_device/include/audio_device.h"
#include "modules/audio_device/include/audio_device_defines.h"
#include "rtc_base/thread.h"
#include "rtc_base/thread_annotations.h"

namespace livekit_ffi {

/// ADM Proxy that manages synthetic and platform audio modes.
///
/// This proxy implements the AudioDeviceModule interface and switches between:
/// 1. **Synthetic mode**: Uses `SyntheticAudioDevice`, which pumps the WebRTC
///    audio pipeline without platform audio. Remote audio is delivered via FFI
///    callbacks to external audio systems (e.g., Unity AudioSource).
/// 2. **Platform mode**: Real audio I/O through the Platform ADM with microphone
///    capture and speaker playout. Used when PlatformAudio is active for VoIP
///    with AEC.
///
/// ## Threading Model
///
/// The proxy is worker-thread-affine. Every public method marshals once at the
/// boundary onto the WebRTC worker thread via `RunOnWorker` and runs inline
/// when the caller is already on the worker, which is the case for all calls
/// originating from WebRTC internals (AudioState, the voice engine). All
/// mutable state, including both sub ADMs, is owned by the worker thread, so
/// no locks are needed and mode transitions execute as plain sequential code.
///
/// This also satisfies the platform ADM threading contract: platform
/// implementations bind a sequence checker to their construction thread and
/// expect every control call on it. The proxy must therefore be constructed
/// on the worker thread, so the eagerly created platform ADM binds to it.
/// Destruction may happen on any thread, teardown hops to the worker.
///
/// Note for future extensions: platform ADM observer callbacks must not call
/// synchronously back into AudioDeviceController, as that would block the
/// observer thread on the worker while the worker may be waiting on it.
///
/// ## Mode Selection
///
/// - **Playout**: Uses Platform ADM when `ref_count > 0 && playout_enabled`,
///   otherwise uses synthetic mode (internal audio pumping task).
/// - **Recording**: Uses Platform ADM when `ref_count > 0 && recording_enabled`,
///   otherwise recording is unavailable (synthetic mode has no microphone).
///
/// ## Lifecycle Management
///
/// Platform ADM is created eagerly at construction (for iOS compatibility).
/// Reference counting controls which mode is active:
/// - `AcquirePlatformAdm()`: Increments ref count
/// - `ReleasePlatformAdm()`: Decrements ref count
/// - When ref_count is 0, playout uses synthetic mode
///
/// ## Audio Modes
///
/// | Mode | Recording | Playout | Use Case |
/// |------|-----------|---------|----------|
/// | Synthetic | NativeAudioSource | Internal task + FFI | Unity audio, agents |
/// | Platform | Platform ADM mic | Platform ADM speakers | VoIP with AEC |
///
class AdmProxy : public webrtc::AudioDeviceModule {
 public:
  /// Must be constructed on the worker thread.
  explicit AdmProxy(const webrtc::Environment& env,
                    webrtc::Thread* worker_thread);
  ~AdmProxy() override;

  // ===========================================================================
  // Platform ADM Lifecycle Management
  // ===========================================================================

  /// Acquires a reference to the Platform ADM.
  ///
  /// On first call, creates and initializes the Platform ADM. On subsequent
  /// calls, just increments the reference count.
  ///
  /// @return true if Platform ADM is ready for use, false if initialization failed.
  bool AcquirePlatformAdm();

  /// Releases a reference to the Platform ADM.
  ///
  /// When the reference count reaches zero, the proxy returns to synthetic
  /// mode. The Platform ADM instance stays alive until destruction.
  void ReleasePlatformAdm();

  /// Returns the current reference count for the Platform ADM.
  int platform_adm_ref_count() const;

  /// Returns true if Platform ADM is currently active (ref_count > 0).
  bool is_platform_adm_active() const;

  // ===========================================================================
  // Recording/Playout Control
  // ===========================================================================

  /// Control whether recording (microphone) is enabled.
  ///
  /// When disabled (default), InitRecording/StartRecording return success but
  /// do nothing. This allows NativeAudioSource to work without interference.
  ///
  /// @note Only effective when Platform ADM is active.
  void set_recording_enabled(bool enabled);
  bool recording_enabled() const;

  /// Control whether playout goes through Platform ADM speakers.
  ///
  /// When disabled (default), playout uses synthetic mode - remote audio is
  /// delivered via FFI callbacks to the application (e.g., Unity AudioSource).
  ///
  /// When enabled, remote audio plays through the platform speakers with AEC.
  ///
  /// @note Only effective when Platform ADM is active.
  void set_playout_enabled(bool enabled);
  bool playout_enabled() const;

  // ===========================================================================
  // AudioDeviceModule Interface
  // ===========================================================================

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
  // Runs fn on the worker thread, inline when already on it.
  template <typename Fn>
  auto RunOnWorker(Fn&& fn) const {
    if (worker_thread_->IsCurrent()) {
      return fn();
    }
    return worker_thread_->BlockingCall(std::forward<Fn>(fn));
  }

  // Forwards a call to the platform ADM on the worker thread.
  // Returns default_value when no platform ADM is available.
  template <typename R, typename Fn>
  R WithPlatformAdm(R default_value, Fn&& fn) const {
    return RunOnWorker([&]() -> R {
      RTC_DCHECK_RUN_ON(worker_thread_);
      if (!platform_adm_) {
        return default_value;
      }
      return fn(*platform_adm_);
    });
  }

  // Returns true if platform mode is active for playout
  // (ref_count > 0 && playout_enabled)
  bool IsPlatformPlayoutActive() const RTC_RUN_ON(worker_thread_);

  // Returns the ADM to use for recording operations.
  // Platform ADM when recording is enabled (ref_count > 0 && recording_enabled),
  // nullptr otherwise (recording not available in synthetic mode).
  webrtc::AudioDeviceModule* RecordingAdm() const RTC_RUN_ON(worker_thread_);

  // Switches playout between synthetic and platform mode based on current
  // state. Called when ref_count or playout_enabled changes.
  // If playout is active, stops the old mode and starts the new one.
  void SwitchPlayoutMode() RTC_RUN_ON(worker_thread_);

  // Switches recording to the correct ADM based on current state.
  // Called when ref_count or recording_enabled changes.
  // If recording is active, stops the old ADM and starts the new one.
  void SwitchRecordingAdm() RTC_RUN_ON(worker_thread_);

#if defined(__ANDROID__)
  // Lazily creates and initializes the Platform ADM on Android.
  // Returns true if ADM is available after the call.
  bool EnsurePlatformAdmCreated() RTC_RUN_ON(worker_thread_);
#endif

  const webrtc::Environment env_;
  webrtc::Thread* const worker_thread_;

  // All mutable state below is owned by the worker thread.

  // Synthetic ADM for synthetic mode - pumps the WebRTC audio pipeline without
  // platform audio via SyntheticAudioDevice's internal task.
  webrtc::scoped_refptr<SyntheticAudioDevice> synthetic_adm_
      RTC_GUARDED_BY(worker_thread_);

  // Platform ADM for real audio I/O (microphone capture, speaker playout with AEC)
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_
      RTC_GUARDED_BY(worker_thread_);

  // Reference count for Platform ADM users (PlatformAudio instances)
  int platform_adm_ref_count_ RTC_GUARDED_BY(worker_thread_) = 0;

  // Audio transport callback (registered by WebRTC)
  webrtc::AudioTransport* audio_transport_ RTC_GUARDED_BY(worker_thread_) =
      nullptr;

  // State tracking
  bool playing_ RTC_GUARDED_BY(worker_thread_) = false;
  bool recording_ RTC_GUARDED_BY(worker_thread_) = false;

  // Control flags
  // When false (default), recording operations are no-ops (NativeAudioSource mode)
  bool recording_enabled_ RTC_GUARDED_BY(worker_thread_) = false;
  // When false (default), playout uses synthetic mode (internal task pumps audio)
  bool playout_enabled_ RTC_GUARDED_BY(worker_thread_) = false;

  // Selected device indices, stored so a selection made before the Platform
  // ADM exists (Android lazy creation) can be re-applied once it is created.
  // Index 0 (the default device) is treated as never explicitly selected.
  uint16_t selected_playout_device_ RTC_GUARDED_BY(worker_thread_) = 0;
  uint16_t selected_recording_device_ RTC_GUARDED_BY(worker_thread_) = 0;
};

}  // namespace livekit_ffi
