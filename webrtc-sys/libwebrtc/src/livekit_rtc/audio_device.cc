#include "livekit_rtc/audio_device.h"

#include <memory>

namespace livekit {

const int kBytesPerSample = 2;
const int kSampleRate = 48000;
const int kChannels = 2;
const int kSamplesPer10Ms = kSampleRate / 100;

int32_t AudioDevice::Init() {
  webrtc::MutexLock lock(&mutex_);
  if (initialized_)
    return 0;

  audio_queue_ = std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>(task_queue_factory_->CreateTaskQueue(
          "AudioDevice", webrtc:: TaskQueueFactory::Priority::NORMAL));

  audio_task_ =
      webrtc::RepeatingTaskHandle::Start(audio_queue_->Current(), [this]() {
        webrtc::MutexLock lock(&mutex_);

        if (playing_) {
          int64_t elapsed_time_ms = -1;
          int64_t ntp_time_ms = -1;
          size_t n_samples_out = 0;
          void* data = data_.data();

          audio_transport_->NeedMorePlayData(
              kSamplesPer10Ms, kBytesPerSample, kChannels, kSampleRate, data,
              n_samples_out, &elapsed_time_ms, &ntp_time_ms);
        }

        return webrtc::TimeDelta::Millis(10);
      });

  initialized_ = true;
  return 0;
}

int32_t AudioDevice::Terminate() {
  {
    webrtc::MutexLock lock(&mutex_);
    if (!initialized_)
      return 0;

    initialized_ = false;
  }
  audio_queue_ = nullptr;
  return 0;
}

}  // namespace livekit
