#ifndef LIVEKIT_AUDIO_DEVICE_H
#define LIVEKIT_AUDIO_DEVICE_H

#include "api/task_queue/task_queue_factory.h"
#include "modules/audio_device/include/audio_device.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/task_queue.h"
#include "rtc_base/task_utils/repeating_task.h"

namespace livekit {

class AudioDevice : public webrtc::AudioDeviceModule {
 public:
  AudioDevice(webrtc::TaskQueueFactory* task_queue_factory)
      : task_queue_factory_(task_queue_factory) {}
  ~AudioDevice() override { Terminate(); }

  int32_t Init() override;
  int32_t Terminate() override;

  int32_t ActiveAudioLayer(AudioLayer* audioLayer) const override {
    *audioLayer = AudioDeviceModule::kDummyAudio;
    return 0;
  }
  int32_t RegisterAudioCallback(webrtc::AudioTransport* transport) override {
    webrtc::MutexLock lock(&mutex_);
    audio_transport_ = transport;
    return 0;
  }

  bool Initialized() const override {
    webrtc::MutexLock lock(&mutex_);
    return initialized_;
  }

  int16_t PlayoutDevices() override { return 0; }
  int16_t RecordingDevices() override { return 0; }
  int32_t PlayoutDeviceName(uint16_t index,
                            char name[webrtc::kAdmMaxDeviceNameSize],
                            char guid[webrtc::kAdmMaxGuidSize]) override {
    return 0;
  }

  int32_t RecordingDeviceName(uint16_t index,
                              char name[webrtc::kAdmMaxDeviceNameSize],
                              char guid[webrtc::kAdmMaxGuidSize]) override {
    return 0;
  }

  int32_t SetPlayoutDevice(uint16_t index) override { return 0; }
  int32_t SetPlayoutDevice(WindowsDeviceType device) override { return 0; }
  int32_t SetRecordingDevice(uint16_t index) override { return 0; }
  int32_t SetRecordingDevice(WindowsDeviceType device) override { return 0; }

  int32_t PlayoutIsAvailable(bool* available) override { return 0; }
  int32_t InitPlayout() override { return 0; }
  bool PlayoutIsInitialized() const override { return false; }
  int32_t RecordingIsAvailable(bool* available) override { return 0; }
  int32_t InitRecording() override { return 0; }
  bool RecordingIsInitialized() const override { return false; }

  int32_t StartPlayout() override {
    webrtc::MutexLock lock(&mutex_);
    playing_ = true;
    return 0;
  }
  int32_t StopPlayout() override {
    webrtc::MutexLock lock(&mutex_);
    playing_ = false;
    return 0;
  }
  bool Playing() const override {
    webrtc::MutexLock lock(&mutex_);
    return playing_;
  }
  int32_t StartRecording() override { return 0; }
  int32_t StopRecording() override { return 0; }
  bool Recording() const override { return false; }

  int32_t InitSpeaker() override { return 0; }
  bool SpeakerIsInitialized() const override { return false; }
  int32_t InitMicrophone() override { return 0; }
  bool MicrophoneIsInitialized() const override { return false; }

  int32_t SpeakerVolumeIsAvailable(bool* available) override { return 0; }
  int32_t SetSpeakerVolume(uint32_t volume) override { return 0; }
  int32_t SpeakerVolume(uint32_t* volume) const override { return 0; }
  int32_t MaxSpeakerVolume(uint32_t* maxVolume) const override { return 0; }
  int32_t MinSpeakerVolume(uint32_t* minVolume) const override { return 0; }

  int32_t MicrophoneVolumeIsAvailable(bool* available) override { return 0; }
  int32_t SetMicrophoneVolume(uint32_t volume) override { return 0; }
  int32_t MicrophoneVolume(uint32_t* volume) const override { return 0; }
  int32_t MaxMicrophoneVolume(uint32_t* maxVolume) const override { return 0; }
  int32_t MinMicrophoneVolume(uint32_t* minVolume) const override { return 0; }

  int32_t SpeakerMuteIsAvailable(bool* available) override { return 0; }
  int32_t SetSpeakerMute(bool enable) override { return 0; }
  int32_t SpeakerMute(bool* enabled) const override { return 0; }

  int32_t MicrophoneMuteIsAvailable(bool* available) override { return 0; }
  int32_t SetMicrophoneMute(bool enable) override { return 0; }
  int32_t MicrophoneMute(bool* enabled) const override { return 0; }

  int32_t StereoPlayoutIsAvailable(bool* available) const override { return 0; }
  int32_t SetStereoPlayout(bool enable) override { return 0; }
  int32_t StereoPlayout(bool* enabled) const override { return 0; }
  int32_t StereoRecordingIsAvailable(bool* available) const override {
    *available = true;
    return 0;
  }
  int32_t SetStereoRecording(bool enable) override { return 0; }
  int32_t StereoRecording(bool* enabled) const override {
    *enabled = true;
    return 0;
  }

  int32_t PlayoutDelay(uint16_t* delayMS) const override { return 0; }

  bool BuiltInAECIsAvailable() const override { return false; }
  bool BuiltInAGCIsAvailable() const override { return false; }
  bool BuiltInNSIsAvailable() const override { return false; }

  int32_t EnableBuiltInAEC(bool enable) override { return 0; }
  int32_t EnableBuiltInAGC(bool enable) override { return 0; }
  int32_t EnableBuiltInNS(bool enable) override { return 0; }

#if defined(WEBRTC_IOS)
  int GetPlayoutAudioParameters(
      webrtc::AudioParameters* params) const override {
    return 0;
  }
  int GetRecordAudioParameters(webrtc::AudioParameters* params) const override {
    return 0;
  }
#endif  // WEBRTC_IOS

  int32_t SetAudioDeviceSink(webrtc::AudioDeviceSink* sink) const override {
    return 0;
  }

 private:
  mutable webrtc::Mutex mutex_;
  std::vector<int16_t> data_;
  std::unique_ptr<rtc::TaskQueue> audio_queue_;
  webrtc::RepeatingTaskHandle audio_task_;
  webrtc::AudioTransport* audio_transport_;
  webrtc::TaskQueueFactory* task_queue_factory_;
  bool playing_{false};
  bool initialized_{false};
};

}  // namespace livekit

#endif  // LIVEKIT_AUDIO_DEVICE_H
