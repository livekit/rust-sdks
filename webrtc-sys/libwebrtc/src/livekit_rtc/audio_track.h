#ifndef LIVEKIT_AUDIO_TRACK_H
#define LIVEKIT_AUDIO_TRACK_H

#include "api/audio/audio_frame.h"
#include "api/audio_options.h"
#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "api/task_queue/task_queue_base.h"
#include "api/task_queue/task_queue_factory.h"
#include "audio/remix_resample.h"
#include "common_audio/resampler/include/push_resampler.h"
#include "livekit_rtc/capi.h"
#include "livekit_rtc/media_stream_track.h"
#include "pc/local_audio_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/task_utils/repeating_task.h"
#include "rtc_base/thread_annotations.h"

namespace livekit {

using CompleteCallback = void (*)(void* userdata);
using AudioDataCallback = void (*)(int16_t* audioData,
                                   uint32_t sampleRate,
                                   uint32_t numberOfChannels,
                                   int numberOfFrames,
                                   void* userdata);

class NativeAudioSink : public webrtc::RefCountInterface {
 protected:
  class InternalSink : public webrtc::AudioTrackSinkInterface {
   public:
    InternalSink(AudioDataCallback callback,
                 void* userdata,
                 int sample_rate,
                 size_t num_channels);

    void OnData(const void* audio_data,
                int bits_per_sample,
                int sample_rate,
                size_t number_of_channels,
                size_t number_of_frames) override;

   private:
    AudioDataCallback callback_;
    void* userdata_;
    int sample_rate_;
    size_t num_channels_;
    webrtc::AudioFrame frame_;
    webrtc::PushResampler<int16_t> resampler_;
    std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>
        audio_queue_;
  };

 public:
  explicit NativeAudioSink(int sample_rate,
                           size_t num_channels,
                           AudioDataCallback callback,
                           void* userdata);

  webrtc::AudioTrackSinkInterface* audio_track_sink() {
    return &internal_sink_;
  }

 private:
  InternalSink internal_sink_;
};

class AudioTrack : public MediaStreamTrack {
 public:
  explicit AudioTrack(webrtc::scoped_refptr<webrtc::AudioTrackInterface> track)
      : MediaStreamTrack(track) {}

  ~AudioTrack() override {
    webrtc::MutexLock lock(&mutex_);
    for (auto& sink : sinks_) {
      audio_track()->RemoveSink(sink->audio_track_sink());
    }
  }

  void add_sink(const webrtc::scoped_refptr<NativeAudioSink>& sink) const {
    webrtc::MutexLock lock(&mutex_);
    audio_track()->AddSink(sink->audio_track_sink());
    sinks_.push_back(sink);
  }

  void remove_sink(const webrtc::scoped_refptr<NativeAudioSink>& sink) const {
    webrtc::MutexLock lock(&mutex_);
    audio_track()->RemoveSink(sink->audio_track_sink());
    sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
  }

  webrtc::AudioTrackInterface* audio_track() const {
    return reinterpret_cast<webrtc::AudioTrackInterface*>(track());
  }

 private:
  mutable webrtc::Mutex mutex_;
  mutable std::vector<webrtc::scoped_refptr<NativeAudioSink>> sinks_
      RTC_GUARDED_BY(mutex_);
};

class AudioTrackSource : public webrtc::RefCountInterface {
  class InternalSource : public webrtc::LocalAudioSource {
   public:
    friend class AudioTrackSource;

   public:
    InternalSource(const webrtc::AudioOptions& options,
                   int sample_rate,
                   int num_channels,
                   int buffer_size_ms,
                   webrtc::TaskQueueFactory* task_queue_factory);

    ~InternalSource() override;

    SourceState state() const override;
    bool remote() const override;

    const webrtc::AudioOptions options() const override;

    void AddSink(webrtc::AudioTrackSinkInterface* sink) override;
    void RemoveSink(webrtc::AudioTrackSinkInterface* sink) override;

    void set_options(const webrtc::AudioOptions& options);

    bool capture_frame(std::vector<int16_t> audio_data,
                       uint32_t sample_rate,
                       uint32_t number_of_channels,
                       size_t number_of_frames,
                       void* userdata,
                       CompleteCallback on_complete);

    void clear_buffer();

   protected:
    int sample_rate_;
    int num_channels_;

   private:
    int queue_size_samples_;
    int notify_threshold_samples_;
    mutable webrtc::Mutex mutex_;
    std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter>
        audio_queue_;
    webrtc::RepeatingTaskHandle audio_task_;

    std::vector<webrtc::AudioTrackSinkInterface*> sinks_ RTC_GUARDED_BY(mutex_);
    std::vector<int16_t> buffer_ RTC_GUARDED_BY(mutex_);

    void* capture_userdata_ RTC_GUARDED_BY(mutex_);
    void (*on_complete_)(void*) RTC_GUARDED_BY(mutex_);

    int missed_frames_ RTC_GUARDED_BY(mutex_) = 0;
    std::vector<int16_t> silence_buffer_;
    webrtc::AudioOptions options_{};
  };

 public:
  AudioTrackSource(lkAudioSourceOptions options,
                   int sample_rate,
                   int num_channels,
                   int queue_size_ms,
                   webrtc::TaskQueueFactory* task_queue_factory);

  static webrtc::scoped_refptr<AudioTrackSource> Create(
      lkAudioSourceOptions options,
      int sample_rate,
      int num_channels,
      int queue_size_ms);

  lkAudioSourceOptions audio_options() const;

  void set_audio_options(const lkAudioSourceOptions& options) const;

  bool capture_frame(std::vector<int16_t> audio_data,
                     uint32_t sample_rate,
                     uint32_t number_of_channels,
                     size_t number_of_frames,
                     void* ctx,
                     CompleteCallback on_complete) const;

  void clear_buffer() const;

  uint32_t sample_rate() const { return source_->sample_rate_; }

  uint32_t num_channels() const { return source_->num_channels_; }

  webrtc::AudioSourceInterface* audio_source() const { return source_.get(); }

  webrtc::scoped_refptr<InternalSource> get() const;

 private:
  webrtc::scoped_refptr<InternalSource> source_;
};

}  // namespace livekit

#endif  // LIVEKIT_AUDIO_TRACK_H
