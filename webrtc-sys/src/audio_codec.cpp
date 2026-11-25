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

#include "livekit/audio_codec.h"
#include "livekit/rtc_error.h"

#include <algorithm>
#include <span>
#include <stdexcept>
#include <utility>

#include "api/audio/audio_view.h"
#include "api/audio_codecs/audio_format.h"
#include "api/audio_codecs/builtin_audio_decoder_factory.h"
#include "api/audio_codecs/builtin_audio_encoder_factory.h"
#include "api/environment/environment_factory.h"
#include "audio/remix_resample.h"
#include "rtc_base/logging.h"

namespace livekit_ffi {
namespace {

constexpr int kOpusSampleRate = 48000;
constexpr int kOpusPayloadType = 111;
// 120 ms (the longest Opus packet) of 48 kHz stereo.
constexpr size_t kMaxDecodedSamples = 120 * 48 * 2;

bool IsSupportedRate(uint32_t sample_rate) {
  return sample_rate >= 8000 && sample_rate <= 96000 && sample_rate % 100 == 0;
}

bool IsSupportedChannelCount(uint32_t num_channels) {
  return num_channels == 1 || num_channels == 2;
}

}  // namespace

// ---------------------------------------------------------------------------
// NativeOpusEncoder
// ---------------------------------------------------------------------------

NativeOpusEncoder::NativeOpusEncoder() : env_(webrtc::CreateEnvironment()) {}

std::string NativeOpusEncoder::Initialize(const OpusEncoderSettings& settings) {
  if (!IsSupportedRate(settings.sample_rate)) {
    return "unsupported input sample rate";
  }
  if (!IsSupportedChannelCount(settings.num_channels)) {
    return "only mono and stereo input is supported";
  }
  const uint32_t frame_duration_ms =
      settings.frame_duration_ms == 0 ? 20 : settings.frame_duration_ms;
  if (frame_duration_ms != 10 && frame_duration_ms != 20 &&
      frame_duration_ms != 40 && frame_duration_ms != 60) {
    return "frame_duration_ms must be 10, 20, 40 or 60";
  }

  webrtc::CodecParameterMap params;
  params["stereo"] = settings.stereo ? "1" : "0";
  params["useinbandfec"] = settings.fec ? "1" : "0";
  params["usedtx"] = settings.dtx ? "1" : "0";
  params["ptime"] = std::to_string(frame_duration_ms);
  params["minptime"] = "10";
  if (settings.bitrate_bps > 0) {
    params["maxaveragebitrate"] = std::to_string(settings.bitrate_bps);
  }
  const webrtc::SdpAudioFormat format("opus", kOpusSampleRate, 2,
                                      std::move(params));

  auto factory = webrtc::CreateBuiltinAudioEncoderFactory();
  webrtc::AudioEncoderFactory::Options options;
  options.payload_type = kOpusPayloadType;
  encoder_ = factory->Create(env_, format, options);
  if (!encoder_) {
    return "the builtin audio encoder factory cannot create Opus";
  }

  // Without a known transport overhead, the Opus encoder ignores bitrate
  // updates. Packets go out without RTP here, so the overhead is zero.
  encoder_->OnReceivedOverhead(0);

  input_sample_rate_ = static_cast<int>(settings.sample_rate);
  input_channels_ = settings.num_channels;
  return "";
}

OpusEncodeResult NativeOpusEncoder::encode(rust::Slice<const int16_t> pcm) {
  OpusEncodeResult result{};
  has_packet_ = false;
  const size_t samples_per_channel =
      static_cast<size_t>(input_sample_rate_ / 100);
  if (pcm.size() != samples_per_channel * input_channels_) {
    return result;
  }

  std::span<const int16_t> audio(pcm.data(), pcm.size());
  const int encoder_rate = encoder_->SampleRateHz();
  const size_t encoder_channels = encoder_->NumChannels();
  if (input_sample_rate_ != encoder_rate ||
      input_channels_ != encoder_channels) {
    resampled_.sample_rate_hz_ = encoder_rate;
    resampled_.num_channels_ = encoder_channels;
    resampled_.samples_per_channel_ =
        webrtc::SampleRateToDefaultChannelSize(encoder_rate);
    webrtc::voe::RemixAndResample(
        webrtc::InterleavedView<const int16_t>(pcm.data(), samples_per_channel,
                                               input_channels_),
        input_sample_rate_, &resampler_, &resampled_);
    audio = std::span<const int16_t>(
        resampled_.data(),
        resampled_.samples_per_channel_ * resampled_.num_channels_);
  }

  const size_t packet_chunks = encoder_->Num10MsFramesInNextPacket();
  rms_level_.Analyze(audio);
  ++analyzed_chunks_;
  encoded_.Clear();
  const webrtc::AudioEncoder::EncodedInfo info =
      encoder_->Encode(next_timestamp_, audio, &encoded_);
  next_timestamp_ += static_cast<uint32_t>(encoder_->RtpTimestampRateHz() / 100);

  result.ok = true;
  // DTX can suppress an entire packet. Start a new measurement window on
  // that boundary as well, so silence cannot dilute the next talk spurt.
  if (info.encoded_bytes != 0 || analyzed_chunks_ >= packet_chunks) {
    result.audio_level = static_cast<uint8_t>(rms_level_.Average()) |
                         (info.speech ? 0x80 : 0);
    analyzed_chunks_ = 0;
  }
  if (info.encoded_bytes == 0) {
    return result;
  }

  encoded_.SetSize(info.encoded_bytes);
  has_packet_ = true;
  result.has_packet = true;
  result.timestamp = info.encoded_timestamp;
  // The packet ends with the chunk just encoded.
  result.samples_per_channel = next_timestamp_ - info.encoded_timestamp;
  return result;
}

void NativeOpusEncoder::set_bitrate(uint32_t bitrate_bps) {
  encoder_->OnReceivedUplinkBandwidth(static_cast<int>(bitrate_bps),
                                      std::nullopt);
}

void NativeOpusEncoder::set_packet_loss_rate(float fraction) {
  encoder_->OnReceivedUplinkPacketLossFraction(fraction);
}

uint32_t NativeOpusEncoder::bitrate() const {
  return static_cast<uint32_t>(encoder_->GetTargetBitrate());
}

std::unique_ptr<NativeOpusEncoder> create_opus_encoder(
    const OpusEncoderSettings& settings) {
  auto encoder = std::make_unique<NativeOpusEncoder>();
  if (std::string error = encoder->Initialize(settings); !error.empty()) {
    throw BridgeException(error);
  }
  return encoder;
}

// ---------------------------------------------------------------------------
// NativeOpusDecoder
// ---------------------------------------------------------------------------

NativeOpusDecoder::NativeOpusDecoder()
    : env_(webrtc::CreateEnvironment()), decoded_(kMaxDecodedSamples) {}

std::string NativeOpusDecoder::Initialize(uint32_t sample_rate,
                                          uint32_t num_channels) {
  if (!IsSupportedRate(sample_rate)) {
    return "unsupported output sample rate";
  }
  if (!IsSupportedChannelCount(num_channels)) {
    return "only mono and stereo output is supported";
  }

  webrtc::CodecParameterMap params;
  params["stereo"] = num_channels == 2 ? "1" : "0";
  const webrtc::SdpAudioFormat format("opus", kOpusSampleRate, 2,
                                      std::move(params));
  auto factory = webrtc::CreateBuiltinAudioDecoderFactory();
  decoder_ = factory->Create(env_, format, std::nullopt);
  if (!decoder_) {
    return "the builtin audio decoder factory cannot create Opus";
  }

  output_sample_rate_ = static_cast<int>(sample_rate);
  output_channels_ = num_channels;
  return "";
}

rust::Slice<const uint8_t> NativeOpusEncoder::packet() const {
  if (!has_packet_) {
    return rust::Slice<const uint8_t>();
  }
  return rust::Slice<const uint8_t>(encoded_.data(), encoded_.size());
}

rust::Slice<const int16_t> NativeOpusDecoder::output() const {
  return rust::Slice<const int16_t>(output_.data(), output_.size());
}

bool NativeOpusDecoder::decode(rust::Slice<const uint8_t> packet) {
  if (packet.empty()) {
    return false;
  }
  webrtc::AudioDecoder::SpeechType speech_type;
  const int samples = decoder_->Decode(
      packet.data(), packet.size(), decoder_->SampleRateHz(),
      decoded_.size() * sizeof(int16_t), decoded_.data(), &speech_type);
  if (samples < 0) {
    return false;
  }
  last_packet_samples_ = static_cast<size_t>(samples) / decoder_->Channels();
  return Output(static_cast<size_t>(samples));
}

bool NativeOpusDecoder::conceal(rust::Slice<const uint8_t> next_packet) {
  webrtc::AudioDecoder::SpeechType speech_type;
  const size_t max_bytes = decoded_.size() * sizeof(int16_t);
  if (!next_packet.empty() &&
      decoder_->PacketHasFec(next_packet.data(), next_packet.size())) {
    // In-band FEC: the next packet carries a low-bitrate copy of this one.
    const int samples = decoder_->DecodeRedundant(
        next_packet.data(), next_packet.size(), decoder_->SampleRateHz(),
        max_bytes, decoded_.data(), &speech_type);
    if (samples > 0) {
      return Output(static_cast<size_t>(samples));
    }
  }
  // Packet loss concealment: Opus extrapolates from the previous packets when
  // asked to decode an empty payload, 10 ms at a time. Conceal as much audio
  // as the last packet carried.
  const size_t channels = decoder_->Channels();
  const size_t wanted = std::min(last_packet_samples_ * channels,
                                 decoded_.size());
  size_t total = 0;
  while (total < wanted) {
    const int samples = decoder_->Decode(
        nullptr, 0, decoder_->SampleRateHz(),
        (decoded_.size() - total) * sizeof(int16_t), decoded_.data() + total,
        &speech_type);
    if (samples <= 0) {
      break;
    }
    total += static_cast<size_t>(samples);
  }
  if (total == 0) {
    return false;
  }
  return Output(total);
}

bool NativeOpusDecoder::Output(size_t total_samples) {
  const size_t decoder_channels = decoder_->Channels();
  const int decoder_rate = decoder_->SampleRateHz();
  const size_t samples_per_channel = total_samples / decoder_channels;

  output_.clear();
  if (decoder_rate == output_sample_rate_ &&
      decoder_channels == output_channels_) {
    output_.assign(decoded_.begin(), decoded_.begin() + total_samples);
    return true;
  }

  // The resampler works on 10 ms chunks.
  const size_t chunk = webrtc::SampleRateToDefaultChannelSize(decoder_rate);
  if (samples_per_channel % chunk != 0) {
    RTC_LOG(LS_WARNING) << "Opus packet of " << samples_per_channel
                        << " samples is not a multiple of 10 ms; cannot "
                           "resample it";
    return false;
  }
  for (size_t offset = 0; offset < samples_per_channel; offset += chunk) {
    resampled_.sample_rate_hz_ = output_sample_rate_;
    resampled_.num_channels_ = output_channels_;
    resampled_.samples_per_channel_ =
        webrtc::SampleRateToDefaultChannelSize(output_sample_rate_);
    webrtc::voe::RemixAndResample(
        webrtc::InterleavedView<const int16_t>(
            decoded_.data() + offset * decoder_channels, chunk,
            decoder_channels),
        decoder_rate, &resampler_, &resampled_);
    const int16_t* data = resampled_.data();
    const size_t count =
        resampled_.samples_per_channel_ * resampled_.num_channels_;
    output_.insert(output_.end(), data, data + count);
  }
  return true;
}

std::unique_ptr<NativeOpusDecoder> create_opus_decoder(uint32_t sample_rate,
                                                       uint32_t num_channels) {
  auto decoder = std::make_unique<NativeOpusDecoder>();
  if (std::string error = decoder->Initialize(sample_rate, num_channels);
      !error.empty()) {
    throw BridgeException(error);
  }
  return decoder;
}

}  // namespace livekit_ffi
