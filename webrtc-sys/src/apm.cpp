#include "livekit/apm.h"

#include <iostream>
#include <memory>

namespace livekit {

AudioProcessingModule::AudioProcessingModule(
    const AudioProcessingConfig& config) {
  apm_ = webrtc::AudioProcessingBuilder().Create();

  apm_->ApplyConfig(config.ToWebrtcConfig());
  apm_->Initialize();
}

int AudioProcessingModule::process_stream(const int16_t* src,
                                          size_t src_len,
                                          int16_t* dst,
                                          size_t dst_len,
                                          int sample_rate,
                                          int num_channels) {
  webrtc::StreamConfig stream_cfg(sample_rate, num_channels);
  return apm_->ProcessStream(src, stream_cfg, stream_cfg, dst);
}

int AudioProcessingModule::process_reverse_stream(const int16_t* src,
                                                  size_t src_len,
                                                  int16_t* dst,
                                                  size_t dst_len,
                                                  int sample_rate,
                                                  int num_channels) {
  webrtc::StreamConfig stream_cfg(sample_rate, num_channels);
  return apm_->ProcessReverseStream(src, stream_cfg, stream_cfg, dst);
}

int AudioProcessingModule::set_stream_delay_ms(int delay_ms) {
  return apm_->set_stream_delay_ms(delay_ms);
}

std::unique_ptr<AudioProcessingModule> create_apm(
    bool echo_canceller_enabled,
    bool gain_controller_enabled,
    bool high_pass_filter_enabled,
    bool noise_suppression_enabled) {
  AudioProcessingConfig config;
  config.echo_canceller_enabled = echo_canceller_enabled;
  config.gain_controller_enabled = gain_controller_enabled;
  config.high_pass_filter_enabled = high_pass_filter_enabled;
  config.noise_suppression_enabled = noise_suppression_enabled;
  return std::make_unique<AudioProcessingModule>(config);
}

bool AudioProcessingModule::create_and_attach_aec_dump(rust::Str file_name,
                                                       int64_t max_log_size_bytes) {
  if (!aec_dump_queue_) {
    aec_dump_queue_ = GetGlobalTaskQueueFactory()->CreateTaskQueue(
        "aec-dump", webrtc::TaskQueueFactory::Priority::LOW);
  }
  return apm_->CreateAndAttachAecDump(
      absl::string_view(file_name.data(), file_name.size()),
      max_log_size_bytes,
      aec_dump_queue_.get());
}

void AudioProcessingModule::detach_aec_dump() {
  apm_->DetachAecDump();
  aec_dump_queue_.reset();
}

}  // namespace livekit
