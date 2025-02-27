#include "livekit/aec.h"

#include <memory>

namespace livekit {

bool SampleRateSupportsMultiBand(int sample_rate_hz) {
  return sample_rate_hz == 32000 || sample_rate_hz == 48000;
}

Aec::Aec(const AecOptions& options) : options_(options) {
  aec3_ = std::make_unique<webrtc::EchoCanceller3>(
      webrtc::EchoCanceller3Config(), absl::nullopt, options.sample_rate,
      options.num_channels, options.num_channels);

  cap_buf_ = std::make_unique<webrtc::AudioBuffer>(
      options.sample_rate, options.num_channels, options.sample_rate,
      options.num_channels, options.sample_rate, options.num_channels);

  rend_buf_ = std::make_unique<webrtc::AudioBuffer>(
      options.sample_rate, options.num_channels, options.sample_rate,
      options.num_channels, options.sample_rate, options.num_channels);
}

Aec::~Aec() = default;

void Aec::cancel_echo(int16_t* cap,
                      size_t cap_len,
                      const int16_t* rend,
                      size_t rend_len) {
  webrtc::StreamConfig stream_cfg(options_.sample_rate, options_.num_channels);

  if (!cap || !rend || cap_len == 0 || rend_len == 0)
    return;

  cap_buf_->CopyFrom(cap, stream_cfg);
  rend_buf_->CopyFrom(rend, stream_cfg);

  if (SampleRateSupportsMultiBand(options_.sample_rate)) {
    cap_buf_->SplitIntoFrequencyBands();
    rend_buf_->SplitIntoFrequencyBands();
  }

  aec3_->AnalyzeCapture(cap_buf_.get());
  aec3_->AnalyzeRender(rend_buf_.get());
  aec3_->ProcessCapture(cap_buf_.get(), false);

  if (SampleRateSupportsMultiBand(options_.sample_rate)) {
    cap_buf_->MergeFrequencyBands();
    rend_buf_->SplitIntoFrequencyBands();
  }

  cap_buf_->CopyTo(stream_cfg, cap);
}

std::unique_ptr<Aec> create_aec(int sample_rate, int num_channels) {
  if (sample_rate != 16000 && sample_rate != 32000 && sample_rate != 48000)
    return nullptr;

  return std::make_unique<Aec>(AecOptions{sample_rate, num_channels});
}

}  // namespace livekit
