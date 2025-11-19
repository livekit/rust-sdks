#include "livekit_rtc/audio_track.h"

#include "livekit_rtc/global_task_queue.h"

namespace livekit {

NativeAudioSink::NativeAudioSink(lkNativeAudioSinkObserver* observer,
                                 void* userdata,
                                 int sample_rate,
                                 size_t num_channels)
    : internal_sink_(observer, userdata, sample_rate, num_channels) {}

NativeAudioSink::InternalSink::InternalSink(lkNativeAudioSinkObserver* observer,
                                            void* userdata,
                                            int sample_rate,
                                            size_t num_channels)
    : userdata_(userdata),
      sample_rate_(sample_rate),
      num_channels_(num_channels) {
  frame_.sample_rate_hz_ = sample_rate;
  frame_.num_channels_ = num_channels;
  frame_.samples_per_channel_ =
      webrtc::SampleRateToDefaultChannelSize(sample_rate);
}

void NativeAudioSink::InternalSink::OnData(const void* audio_data,
                                           int bits_per_sample,
                                           int sample_rate,
                                           size_t number_of_channels,
                                           size_t number_of_frames) {
  RTC_CHECK_EQ(16, bits_per_sample);

  const int16_t* data = static_cast<const int16_t*>(audio_data);

  if (sample_rate_ != sample_rate || num_channels_ != number_of_channels) {
    webrtc::InterleavedView<const int16_t> source(data, number_of_frames,
                                                  number_of_channels);
    // resample/remix before capturing
    webrtc::voe::RemixAndResample(source, sample_rate, &resampler_, &frame_);

    observer_->onAudioData((int16_t*)frame_.data(), frame_.sample_rate_hz(),
                           frame_.num_channels(), frame_.samples_per_channel(),
                           userdata_);

    // std::vector<int16_t> slice(
    //     (int16_t *)frame_.data(), frame_.num_channels() *
    //     frame_.samples_per_channel());

    // observer_->on_data(slice, frame_.sample_rate_hz(),
    //                    frame_.num_channels(), frame_.samples_per_channel());

  } else {
    // std::vector<int16_t> rust_slice(data,
    //                                number_of_channels * number_of_frames);

    // observer_->on_data(slice, sample_rate, number_of_channels,
    //                    number_of_frames);

    observer_->onAudioData((int16_t*)data, frame_.sample_rate_hz(),
                           frame_.num_channels(), frame_.samples_per_channel(),
                           userdata_);
  }
}

AudioTrackSource::InternalSource::InternalSource(
    const webrtc::AudioOptions& options,
    int sample_rate,
    int num_channels,
    int queue_size_ms,  // must be a multiple of 10ms
    webrtc::TaskQueueFactory* task_queue_factory)
    : sample_rate_(sample_rate),
      num_channels_(num_channels),
      capture_userdata_(nullptr),
      on_complete_(nullptr) {
  if (!queue_size_ms)
    return;  // no audio queue

  // start sending silence when there is nothing on the queue for 10 frames
  // (100ms)
  const int silence_frames_threshold = 10;
  missed_frames_ = silence_frames_threshold;

  int samples10ms = sample_rate / 100 * num_channels;

  silence_buffer_.assign(samples10ms, 0);
  queue_size_samples_ = queue_size_ms / 10 * samples10ms;
  notify_threshold_samples_ = queue_size_samples_;  // TODO: this is currently
                                                    // using x2 the queue size
  buffer_.reserve(queue_size_samples_ + notify_threshold_samples_);

  audio_queue_ = task_queue_factory->CreateTaskQueue(
      "AudioSourceCapture", webrtc::TaskQueueFactory::Priority::NORMAL);

  audio_task_ = webrtc::RepeatingTaskHandle::Start(
      audio_queue_.get(),
      [this, samples10ms]() {
        webrtc::MutexLock lock(&mutex_);
        constexpr int kBitsPerSample = sizeof(int16_t) * 8;

        if (buffer_.size() >= samples10ms) {
          // Reset |missed_frames_| to 0 so that it won't keep sending silence
          // to webrtc due to audio callback timing drifts.
          missed_frames_ = 0;
          for (auto sink : sinks_)
            sink->OnData(buffer_.data(), kBitsPerSample, sample_rate_,
                         num_channels_, samples10ms / num_channels_);

          buffer_.erase(buffer_.begin(), buffer_.begin() + samples10ms);
        } else {
          missed_frames_++;
          if (missed_frames_ >= silence_frames_threshold) {
            for (auto sink : sinks_)
              sink->OnData(silence_buffer_.data(), kBitsPerSample, sample_rate_,
                           num_channels_, samples10ms / num_channels_);
          }
        }

        if (on_complete_ && buffer_.size() <= notify_threshold_samples_) {
          on_complete_(capture_userdata_);
          on_complete_ = nullptr;
          capture_userdata_ = nullptr;
        }

        return webrtc::TimeDelta::Millis(10);
      },
      webrtc::TaskQueueBase::DelayPrecision::kHigh);
}

AudioTrackSource::InternalSource::~InternalSource() {}

inline lkAudioSourceOptions to_capi_audio_options(
    const webrtc::AudioOptions& rtc_options) {
  lkAudioSourceOptions options{};
  options.echoCancellation = rtc_options.echo_cancellation.value_or(false);
  options.noiseSuppression = rtc_options.noise_suppression.value_or(false);
  options.autoGainControl = rtc_options.auto_gain_control.value_or(false);
  return options;
}

inline webrtc::AudioOptions to_native_audio_options(
    const lkAudioSourceOptions& options) {
  webrtc::AudioOptions rtc_options{};
  rtc_options.echo_cancellation = options.echoCancellation;
  rtc_options.noise_suppression = options.noiseSuppression;
  rtc_options.auto_gain_control = options.autoGainControl;
  return rtc_options;
}

bool AudioTrackSource::InternalSource::capture_frame(
    std::vector<int16_t> data,
    uint32_t sample_rate,
    uint32_t number_of_channels,
    size_t number_of_frames,
    void* ctx,
    CompleteCallback on_complete) {
  webrtc::MutexLock lock(&mutex_);

  if (queue_size_samples_) {
    int available =
        (queue_size_samples_ + notify_threshold_samples_) - buffer_.size();
    if (available < data.size())
      return false;

    if (on_complete_ || capture_userdata_)
      return false;

    buffer_.insert(buffer_.end(), data.begin(), data.end());

    if (buffer_.size() <= notify_threshold_samples_) {
      audio_queue_->PostTask([this, ctx, on_complete]() {
        webrtc::MutexLock lock(&mutex_);
        on_complete(ctx);
      });
    } else {
      on_complete_ = on_complete;
      capture_userdata_ = ctx;
    }

  } else {
    // capture directly when the queue buffer is 0 (frame size must be 10ms)
    for (auto sink : sinks_)
      sink->OnData(data.data(), sizeof(int16_t) * 8, sample_rate,
                   number_of_channels, number_of_frames);
  }

  return true;
}

void AudioTrackSource::InternalSource::clear_buffer() {
  webrtc::MutexLock lock(&mutex_);
  buffer_.clear();
}

webrtc::MediaSourceInterface::SourceState
AudioTrackSource::InternalSource::state() const {
  return webrtc::MediaSourceInterface::SourceState::kLive;
}

bool AudioTrackSource::InternalSource::remote() const {
  return false;
}

const webrtc::AudioOptions AudioTrackSource::InternalSource::options() const {
  webrtc::MutexLock lock(&mutex_);
  return options_;
}

void AudioTrackSource::InternalSource::set_options(
    const webrtc::AudioOptions& options) {
  webrtc::MutexLock lock(&mutex_);
  options_ = options;
}

void AudioTrackSource::InternalSource::AddSink(
    webrtc::AudioTrackSinkInterface* sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.push_back(sink);
}

void AudioTrackSource::InternalSource::RemoveSink(
    webrtc::AudioTrackSinkInterface* sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.erase(std::remove(sinks_.begin(), sinks_.end(), sink), sinks_.end());
}

AudioTrackSource::AudioTrackSource(lkAudioSourceOptions options,
                                   int sample_rate,
                                   int num_channels,
                                   int queue_size_ms,
                                   webrtc::TaskQueueFactory* task_queue_factory)
    : source_(webrtc::make_ref_counted<InternalSource>(
          to_native_audio_options(options),
          sample_rate,
          num_channels,
          queue_size_ms,
          task_queue_factory)) {}

lkAudioSourceOptions AudioTrackSource::audio_options() const {
  return to_capi_audio_options(source_->options());
}

void AudioTrackSource::set_audio_options(
    const lkAudioSourceOptions& options) const {
  source_->set_options(to_native_audio_options(options));
}

bool AudioTrackSource::capture_frame(std::vector<int16_t> audio_data,
                                     uint32_t sample_rate,
                                     uint32_t number_of_channels,
                                     size_t number_of_frames,
                                    void* ctx,
                                     CompleteCallback on_complete) const {
  return source_->capture_frame(audio_data, sample_rate, number_of_channels,
                                number_of_frames, ctx, on_complete);
}

void AudioTrackSource::clear_buffer() const {
  source_->clear_buffer();
}

webrtc::scoped_refptr<AudioTrackSource> AudioTrackSource::Create(
    lkAudioSourceOptions options,
    int sample_rate,
    int num_channels,
    int queue_size_ms) {
  return webrtc::make_ref_counted<AudioTrackSource>(
      options, sample_rate, num_channels, queue_size_ms,
      GetGlobalTaskQueueFactory());
}

webrtc::scoped_refptr<AudioTrackSource::InternalSource> AudioTrackSource::get()
    const {
  return source_;
}

}  // namespace livekit