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

#include "livekit/video_codec.h"
#include "livekit/rtc_error.h"

#include <algorithm>
#include <cmath>
#include <deque>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <thread>
#include <utility>

#include "absl/strings/match.h"
#include "api/environment/environment_factory.h"
#include "api/task_queue/task_queue_factory.h"
#include "api/video/encoded_image.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_bitrate_allocation.h"
#include "api/video_codecs/h264_profile_level_id.h"
#include "common_video/h264/sps_vui_rewriter.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/sdp_video_format.h"
#include "livekit/encoded_video_frame_buffer.h"
#include "livekit/video_decoder_factory.h"
#include "livekit/video_encoder_factory.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "modules/video_coding/codecs/interface/common_constants.h"
#include "modules/video_coding/codecs/vp8/include/vp8.h"
#include "modules/video_coding/codecs/vp9/include/vp9.h"
#include "modules/video_coding/include/video_codec_initializer.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/event.h"
#include "rtc_base/logging.h"
#include "rtc_base/ref_counted_object.h"
#include "rtc_base/time_utils.h"
#include "video/config/video_encoder_config.h"

#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
#include "modules/video_coding/codecs/av1/dav1d_decoder.h"  // nogncheck
#endif

namespace livekit_ffi {
namespace {

// Encodes queued beyond the one being encoded are dropped: raw frames can be
// skipped without breaking the bitstream, and this bounds the queue.
constexpr int kMaxPendingEncodes = 2;
// Encoded frames cannot be dropped without breaking the reference chain, so
// allow a deeper queue before giving up and asking for a keyframe.
constexpr int kMaxPendingDecodes = 16;
constexpr size_t kMaxPayloadSize = 1200;
constexpr uint32_t kMinLayerBitrateBps = 30'000;
constexpr uint32_t kMaxTemporalLayers = 3;
// Encode/decode start times remembered to time the output (frames in flight
// are bounded by the pending encode/decode limits, plus hardware pipelining).
constexpr size_t kMaxTimedFrames = 32;

// Cumulative share of a stream's bitrate given to its temporal layers, as
// WebRTC's simulcast rate allocator splits it (e.g. 40/20/40 % for three).
constexpr float kTemporalLayerRateShare[kMaxTemporalLayers][kMaxTemporalLayers] =
    {{1.0f, 1.0f, 1.0f}, {0.6f, 1.0f, 1.0f}, {0.4f, 0.6f, 1.0f}};

// Remembers when frames (by RTP timestamp) started encoding or decoding.
class FrameTimer {
 public:
  void Start(uint32_t rtp_timestamp) {
    webrtc::MutexLock lock(&mutex_);
    if (starts_.size() >= kMaxTimedFrames) {
      starts_.pop_front();
    }
    starts_.emplace_back(rtp_timestamp, webrtc::TimeMicros());
  }

  // Microseconds since `rtp_timestamp` started, or 0 when it is unknown.
  uint32_t Elapsed(uint32_t rtp_timestamp) {
    webrtc::MutexLock lock(&mutex_);
    for (auto it = starts_.rbegin(); it != starts_.rend(); ++it) {
      if (it->first == rtp_timestamp) {
        const int64_t elapsed = webrtc::TimeMicros() - it->second;
        return static_cast<uint32_t>(
            std::clamp<int64_t>(elapsed, 0, UINT32_MAX));
      }
    }
    return 0;
  }

 private:
  webrtc::Mutex mutex_;
  std::deque<std::pair<uint32_t, int64_t>> starts_ RTC_GUARDED_BY(mutex_);
};

int TemporalId(const webrtc::EncodedImage& image,
               const webrtc::CodecSpecificInfo* info) {
  if (auto index = image.TemporalIndex()) {
    return *index;
  }
  if (!info) {
    return -1;
  }
  switch (info->codecType) {
    case webrtc::kVideoCodecVP8:
      return info->codecSpecific.VP8.temporalIdx == webrtc::kNoTemporalIdx
                 ? -1
                 : info->codecSpecific.VP8.temporalIdx;
    case webrtc::kVideoCodecVP9:
      return info->codecSpecific.VP9.temporal_idx == webrtc::kNoTemporalIdx
                 ? -1
                 : info->codecSpecific.VP9.temporal_idx;
    default:
      break;
  }
  if (info->generic_frame_info) {
    return info->generic_frame_info->temporal_id;
  }
  return -1;
}

std::atomic<int64_t> g_live_video_codecs{0};

int NumberOfCores() {
  return std::max(1u, std::thread::hardware_concurrency());
}

webrtc::VideoCodecType ToCodecType(EncodedVideoCodec codec) {
  switch (codec) {
    case EncodedVideoCodec::H264:
      return webrtc::kVideoCodecH264;
    case EncodedVideoCodec::H265:
      return webrtc::kVideoCodecH265;
    case EncodedVideoCodec::VP8:
      return webrtc::kVideoCodecVP8;
    case EncodedVideoCodec::VP9:
      return webrtc::kVideoCodecVP9;
    case EncodedVideoCodec::AV1:
      return webrtc::kVideoCodecAV1;
  }
  return webrtc::kVideoCodecGeneric;
}

const char* CodecName(EncodedVideoCodec codec) {
  switch (codec) {
    case EncodedVideoCodec::H264:
      return webrtc::kH264CodecName;
    case EncodedVideoCodec::H265:
      return webrtc::kH265CodecName;
    case EncodedVideoCodec::VP8:
      return webrtc::kVp8CodecName;
    case EncodedVideoCodec::VP9:
      return webrtc::kVp9CodecName;
    case EncodedVideoCodec::AV1:
      return webrtc::kAv1CodecName;
  }
  return "";
}

bool MatchesCodecName(std::string_view name, EncodedVideoCodec codec) {
  if (absl::EqualsIgnoreCase(name, CodecName(codec))) {
    return true;
  }
  return codec == EncodedVideoCodec::H265 &&
         absl::EqualsIgnoreCase(name, "HEVC");
}

bool IsCodec(EncodedVideoCodec codec) {
  switch (codec) {
    case EncodedVideoCodec::H264:
    case EncodedVideoCodec::H265:
    case EncodedVideoCodec::VP8:
    case EncodedVideoCodec::VP9:
    case EncodedVideoCodec::AV1:
      return true;
  }
  return false;
}

// Factories are shared by every live codec, and released with the last one:
// creating them probes hardware (and the H.264 decoder), which is too costly
// to repeat per codec, while keeping them forever would pin native resources.
template <typename Factory>
std::shared_ptr<Factory> SharedFactory() {
  static webrtc::Mutex mutex;
  static std::weak_ptr<Factory> weak_factory;
  webrtc::MutexLock lock(&mutex);
  if (auto factory = weak_factory.lock()) {
    return factory;
  }
  auto factory = std::make_shared<Factory>();
  weak_factory = factory;
  return factory;
}

// Smallest H.264 level (at least 3.1, the usual WebRTC default) whose frame
// size and macroblock rate limits fit the stream.
webrtc::H264Level H264LevelFor(int width, int height, double fps) {
  struct Limits {
    webrtc::H264Level level;
    int max_frame_size_mbs;
    double max_mbs_per_second;
  };
  static constexpr Limits kLevels[] = {
      {webrtc::H264Level::kLevel3_1, 3600, 108000},
      {webrtc::H264Level::kLevel3_2, 5120, 216000},
      {webrtc::H264Level::kLevel4, 8192, 245760},
      {webrtc::H264Level::kLevel4_2, 8704, 522240},
      {webrtc::H264Level::kLevel5, 22080, 589824},
      {webrtc::H264Level::kLevel5_1, 36864, 983040},
      {webrtc::H264Level::kLevel5_2, 36864, 2073600},
  };
  const int mbs = ((width + 15) / 16) * ((height + 15) / 16);
  for (const auto& limits : kLevels) {
    if (mbs <= limits.max_frame_size_mbs &&
        mbs * fps <= limits.max_mbs_per_second) {
      return limits.level;
    }
  }
  return webrtc::H264Level::kLevel5_2;
}

// Picks the factory format used to create the encoder. For H.264 this
// prefers packetization-mode=1 and the constrained baseline profile (decodable
// by every receiver), raising the level when the resolution requires it.
std::optional<webrtc::SdpVideoFormat> PickEncoderFormat(
    const std::vector<webrtc::SdpVideoFormat>& formats,
    const VideoEncoderSettings& settings) {
  std::optional<webrtc::SdpVideoFormat> picked;
  int picked_score = -1;
  for (const auto& format : formats) {
    if (!MatchesCodecName(format.name, settings.codec)) {
      continue;
    }
    if (settings.codec != EncodedVideoCodec::H264) {
      picked = format;
      break;
    }
    auto mode = format.parameters.find(webrtc::kH264FmtpPacketizationMode);
    const bool non_interleaved =
        mode != format.parameters.end() && mode->second == "1";
    auto profile = webrtc::ParseSdpForH264ProfileLevelId(format.parameters);
    const bool constrained_baseline =
        profile &&
        profile->profile == webrtc::H264Profile::kProfileConstrainedBaseline;
    const int score = (non_interleaved ? 2 : 0) + (constrained_baseline ? 1 : 0);
    if (score > picked_score) {
      picked = format;
      picked_score = score;
    }
  }

  if (picked && settings.codec == EncodedVideoCodec::H264) {
    auto profile = webrtc::ParseSdpForH264ProfileLevelId(picked->parameters);
    if (profile) {
      const auto level = H264LevelFor(static_cast<int>(settings.width),
                                      static_cast<int>(settings.height),
                                      settings.max_framerate);
      if (static_cast<int>(level) > static_cast<int>(profile->level)) {
        profile->level = level;
        if (auto str = webrtc::H264ProfileLevelIdToString(*profile)) {
          picked->parameters[webrtc::kH264FmtpProfileLevelId] = *str;
        }
      }
    }
  }
  return picked;
}

std::string ValidateSettings(const VideoEncoderSettings& settings) {
  if (!IsCodec(settings.codec)) {
    return "unknown codec";
  }
  if (settings.backend == VideoEncoderBackend::PreEncoded) {
    return "the PreEncoded backend cannot encode raw frames";
  }
  if (settings.width == 0 || settings.height == 0 ||
      settings.width > UINT16_MAX || settings.height > UINT16_MAX) {
    return "invalid resolution";
  }
  if (!(settings.max_framerate > 0.0)) {
    return "max_framerate must be positive";
  }
  if (settings.layers.size() > webrtc::kMaxSimulcastStreams) {
    return "too many simulcast layers (max " +
           std::to_string(webrtc::kMaxSimulcastStreams) + ")";
  }
  if (settings.num_temporal_layers > kMaxTemporalLayers) {
    return "at most 3 temporal layers are supported";
  }
  if (settings.num_temporal_layers > 1 &&
      (settings.codec == EncodedVideoCodec::H264 ||
       settings.codec == EncodedVideoCodec::H265)) {
    return "temporal layers need VP8, VP9 or AV1";
  }
  if (settings.layers.empty()) {
    if (settings.max_bitrate_bps == 0) {
      return "max_bitrate_bps must be positive";
    }
    return "";
  }
  for (size_t i = 0; i < settings.layers.size(); ++i) {
    const auto& layer = settings.layers[i];
    if (layer.width == 0 || layer.height == 0 || layer.max_bitrate_bps == 0) {
      return "simulcast layer " + std::to_string(i) +
             " needs a resolution and max_bitrate_bps";
    }
    if (i > 0 && (layer.width < settings.layers[i - 1].width ||
                  layer.height < settings.layers[i - 1].height)) {
      return "simulcast layers must be ordered from lowest to highest "
             "resolution";
    }
  }
  const auto& top = settings.layers.back();
  if (top.width != settings.width || top.height != settings.height) {
    return "the last simulcast layer must match the encoder resolution";
  }
  return "";
}

// Builds the VideoCodec WebRTC itself would use for these streams.
webrtc::VideoCodec BuildVideoCodec(const webrtc::Environment& env,
                                   const VideoEncoderSettings& settings,
                                   const webrtc::SdpVideoFormat& format) {
  const webrtc::VideoCodecType type = ToCodecType(settings.codec);
  const int max_qp = (type == webrtc::kVideoCodecH264 ||
                      type == webrtc::kVideoCodecH265)
                         ? 51
                         : 56;
  const int temporal_layers =
      static_cast<int>(std::max<uint32_t>(1, settings.num_temporal_layers));
  const webrtc::ScalabilityMode scalability_mode =
      temporal_layers == 3   ? webrtc::ScalabilityMode::kL1T3
      : temporal_layers == 2 ? webrtc::ScalabilityMode::kL1T2
                             : webrtc::ScalabilityMode::kL1T1;

  auto make_stream = [&](uint32_t width, uint32_t height, double fps,
                         uint32_t max_bitrate_bps, uint32_t min_bitrate_bps) {
    webrtc::VideoStream stream;
    stream.width = width;
    stream.height = height;
    stream.max_framerate = std::max(1, static_cast<int>(std::lround(fps)));
    stream.max_bitrate_bps = static_cast<int>(max_bitrate_bps);
    stream.target_bitrate_bps = static_cast<int>(max_bitrate_bps);
    stream.min_bitrate_bps =
        static_cast<int>(std::min(min_bitrate_bps, max_bitrate_bps));
    stream.max_qp = max_qp;
    stream.num_temporal_layers = temporal_layers;
    if (temporal_layers > 1) {
      stream.scalability_mode = scalability_mode;
    }
    // Every stream is configured active; pausing is driven by SetRates so a
    // layer can be (re)activated without re-initializing the encoder.
    stream.active = true;
    return stream;
  };

  std::vector<webrtc::VideoStream> streams;
  if (settings.layers.empty()) {
    streams.push_back(make_stream(settings.width, settings.height,
                                  settings.max_framerate,
                                  settings.max_bitrate_bps,
                                  settings.min_bitrate_bps));
  } else {
    for (size_t i = 0; i < settings.layers.size(); ++i) {
      const auto& layer = settings.layers[i];
      const double fps = layer.max_framerate > 0.0
                             ? std::min(layer.max_framerate,
                                        settings.max_framerate)
                             : settings.max_framerate;
      streams.push_back(make_stream(
          layer.width, layer.height, fps, layer.max_bitrate_bps,
          i == 0 ? settings.min_bitrate_bps : kMinLayerBitrateBps));
    }
  }

  webrtc::VideoEncoderConfig config;
  config.codec_type = type;
  config.video_format = format;
  config.content_type = webrtc::VideoEncoderConfig::ContentType::kRealtimeVideo;
  // Predictable output: one encoded frame per input frame and active layer.
  config.frame_drop_enabled = false;
  config.number_of_streams = streams.size();
  config.simulcast_layers = streams;
  config.max_qp = max_qp;
  config.legacy_conference_mode = false;
  config.is_quality_scaling_allowed = false;
  int max_bitrate_bps = 0;
  for (const auto& stream : streams) {
    max_bitrate_bps += stream.max_bitrate_bps;
  }
  config.max_bitrate_bps = max_bitrate_bps;

  // Encoder-internal resizing would change the output resolution behind the
  // caller's back.
  switch (type) {
    case webrtc::kVideoCodecVP8: {
      webrtc::VideoCodecVP8 vp8 = webrtc::VideoEncoder::GetDefaultVp8Settings();
      vp8.automaticResizeOn = false;
      vp8.numberOfTemporalLayers = temporal_layers;
      config.encoder_specific_settings = webrtc::make_ref_counted<
          webrtc::VideoEncoderConfig::Vp8EncoderSpecificSettings>(vp8);
      break;
    }
    case webrtc::kVideoCodecVP9: {
      webrtc::VideoCodecVP9 vp9 = webrtc::VideoEncoder::GetDefaultVp9Settings();
      vp9.automaticResizeOn = false;
      vp9.numberOfSpatialLayers = 1;
      vp9.numberOfTemporalLayers = temporal_layers;
      vp9.flexibleMode = false;
      config.encoder_specific_settings = webrtc::make_ref_counted<
          webrtc::VideoEncoderConfig::Vp9EncoderSpecificSettings>(vp9);
      break;
    }
    case webrtc::kVideoCodecAV1: {
      webrtc::VideoCodecAV1 av1{};
      av1.automatic_resize_on = false;
      config.encoder_specific_settings = webrtc::make_ref_counted<
          webrtc::VideoEncoderConfig::Av1EncoderSpecificSettings>(av1);
      break;
    }
    default:
      break;
  }

  webrtc::VideoCodec codec = webrtc::VideoCodecInitializer::SetupCodec(
      env.field_trials(), config, streams);

  const uint32_t start_bitrate_bps = settings.start_bitrate_bps > 0
                                         ? settings.start_bitrate_bps
                                         : static_cast<uint32_t>(max_bitrate_bps);
  codec.startBitrate =
      std::clamp(start_bitrate_bps / 1000, codec.minBitrate, codec.maxBitrate);
  codec.active = true;
  for (int i = 0; i < codec.numberOfSimulcastStreams; ++i) {
    codec.simulcastStream[i].active = true;
    codec.simulcastStream[i].numberOfTemporalLayers = temporal_layers;
  }
  if (temporal_layers > 1) {
    switch (type) {
      case webrtc::kVideoCodecVP8:
        codec.VP8()->numberOfTemporalLayers = temporal_layers;
        break;
      case webrtc::kVideoCodecVP9:
        codec.VP9()->numberOfTemporalLayers = temporal_layers;
        break;
      default:
        break;
    }
    if (codec.numberOfSimulcastStreams <= 1) {
      codec.SetScalabilityMode(scalability_mode);
    }
  }
  codec.SetFrameDropEnabled(false);
  return codec;
}

}  // namespace

// ---------------------------------------------------------------------------
// NativeVideoEncoder
// ---------------------------------------------------------------------------

class NativeVideoEncoder::Callback : public webrtc::EncodedImageCallback {
 public:
  Callback(rust::Box<VideoEncoderObserver> observer,
           std::shared_ptr<std::atomic<uint32_t>> keyframe_requests)
      : observer_(std::move(observer)),
        keyframe_requests_(std::move(keyframe_requests)) {}

  // Configured stream sizes, used when an encoder leaves the encoded size
  // unset, and the codec. Called once before the encoder can produce output.
  void Configure(std::vector<std::pair<uint32_t, uint32_t>> sizes,
                 webrtc::VideoCodecType codec_type) {
    webrtc::MutexLock lock(&mutex_);
    stream_sizes_ = std::move(sizes);
    codec_type_ = codec_type;
  }

  // Called on the encoder queue right before `rtp_timestamp` is encoded.
  void OnEncodeStarted(uint32_t rtp_timestamp) { timer_.Start(rtp_timestamp); }

  Result OnEncodedImage(const webrtc::EncodedImage& image,
                        const webrtc::CodecSpecificInfo* codec_info) override {
    if (image.size() == 0 || image.data() == nullptr) {
      return Result(Result::OK);
    }
    const uint32_t encode_time_us = timer_.Elapsed(image.RtpTimestamp());
    std::span<const uint8_t> data(image.data(), image.size());
    webrtc::Buffer rewritten;
    const bool h264 = codec_info ? codec_info->codecType == webrtc::kVideoCodecH264
                                 : codec_type_ == webrtc::kVideoCodecH264;
    if (h264 && image.IsKey()) {
      // As WebRTC's video stream encoder does for every sender: receivers
      // rewrite SPSs without bitstream restrictions, so frame encryption
      // (which authenticates the parameter sets) needs them rewritten first.
      rewritten = webrtc::SpsVuiRewriter::ParseOutgoingBitstreamAndRewrite(
          data, image.ColorSpace());
      data = std::span<const uint8_t>(rewritten.data(), rewritten.size());
    }
    webrtc::MutexLock lock(&mutex_);
    if (detached_) {
      return Result(Result::OK);
    }

    const int layer = std::clamp(
        image.SimulcastIndex().value_or(image.SpatialIndex().value_or(0)), 0,
        static_cast<int>(webrtc::kMaxSimulcastStreams) - 1);
    EncodedVideoFrameInfo info;
    info.layer = static_cast<uint32_t>(layer);
    info.is_keyframe = image.IsKey();
    info.width = image._encodedWidth;
    info.height = image._encodedHeight;
    if ((info.width == 0 || info.height == 0) &&
        static_cast<size_t>(layer) < stream_sizes_.size()) {
      info.width = stream_sizes_[layer].first;
      info.height = stream_sizes_[layer].second;
    }
    info.rtp_timestamp = image.RtpTimestamp();
    info.qp = image.qp_;
    info.encode_time_us = encode_time_us;
    info.temporal_id = TemporalId(image, codec_info);

    if (!observer_->on_encoded_frame(
            info, rust::Slice<const uint8_t>(data.data(), data.size()))) {
      // The receiver dropped this frame; later frames of the layer may
      // reference it, so restart the layer from a keyframe.
      keyframe_requests_->fetch_or(1u << layer);
    }
    return Result(Result::OK);
  }

  void OnFrameDropped(uint32_t, int, bool) override {}

  // After this returns, no output reaches the observer anymore.
  void Detach() {
    webrtc::MutexLock lock(&mutex_);
    detached_ = true;
  }

 private:
  webrtc::Mutex mutex_;
  bool detached_ RTC_GUARDED_BY(mutex_) = false;
  std::vector<std::pair<uint32_t, uint32_t>> stream_sizes_
      RTC_GUARDED_BY(mutex_);
  rust::Box<VideoEncoderObserver> observer_;
  std::shared_ptr<std::atomic<uint32_t>> keyframe_requests_;
  FrameTimer timer_;
  // Set before output starts, read-only after.
  webrtc::VideoCodecType codec_type_ = webrtc::kVideoCodecGeneric;
};

NativeVideoEncoder::NativeVideoEncoder(rust::Box<VideoEncoderObserver> observer)
    : env_(webrtc::CreateEnvironment()),
      keyframe_requests_(std::make_shared<std::atomic<uint32_t>>(0)) {
  callback_ =
      std::make_unique<Callback>(std::move(observer), keyframe_requests_);
  g_live_video_codecs.fetch_add(1);
}

NativeVideoEncoder::~NativeVideoEncoder() {
  // Stop delivering output first, so the observer is never called once the
  // owner started dropping this encoder.
  callback_->Detach();
  if (queue_) {
    webrtc::Event done;
    queue_->PostTask([this, &done] {
      if (encoder_) {
        encoder_->Release();
        encoder_ = nullptr;
      }
      done.Set();
    });
    done.Wait(webrtc::Event::kForever);
    // The queue is idle (the teardown task was the last one posted), so
    // deleting it does not race with pending work.
    queue_ = nullptr;
  }
  g_live_video_codecs.fetch_sub(1);
}

std::string NativeVideoEncoder::Initialize(
    const VideoEncoderSettings& settings) {
  if (std::string error = ValidateSettings(settings); !error.empty()) {
    return error;
  }

  factory_ = SharedFactory<VideoEncoderFactory>();
  auto format = PickEncoderFormat(factory_->GetSupportedFormats(), settings);
  if (!format) {
    return std::string("no ") + CodecName(settings.codec) +
           " encoder is available";
  }

  // The adapter creates each stream's encoder from the format recorded in
  // the codec, so it must carry the backend preference.
  const webrtc::SdpVideoFormat tagged =
      WithVideoEncoderBackend(*format, settings.backend);
  codec_ = BuildVideoCodec(env_, settings, tagged);
  num_streams_ = std::max<int>(1, codec_.numberOfSimulcastStreams);
  num_temporal_layers_ = static_cast<int>(
      std::clamp<uint32_t>(settings.num_temporal_layers, 1, kMaxTemporalLayers));
  width_ = static_cast<int>(settings.width);
  height_ = static_cast<int>(settings.height);

  std::vector<std::pair<uint32_t, uint32_t>> stream_sizes;
  if (settings.layers.empty()) {
    stream_sizes.emplace_back(settings.width, settings.height);
  } else {
    for (const auto& layer : settings.layers) {
      stream_sizes.emplace_back(layer.width, layer.height);
    }
  }
  callback_->Configure(std::move(stream_sizes), codec_.codecType);

  {
    // Initial rates: the start bitrate is allocated to the active layers
    // from the lowest one up, each capped at its maximum.
    webrtc::MutexLock lock(&rates_mutex_);
    framerate_fps_ = settings.max_framerate;
    uint32_t remaining = codec_.startBitrate * 1000;
    if (settings.layers.empty()) {
      layer_bitrates_bps_ = {remaining};
      layer_active_ = {true};
    } else {
      for (const auto& layer : settings.layers) {
        const uint32_t bitrate =
            layer.active ? std::min(remaining, layer.max_bitrate_bps) : 0;
        remaining -= bitrate;
        // Inactive layers keep their maximum, used once activated.
        layer_bitrates_bps_.push_back(layer.active ? bitrate
                                                   : layer.max_bitrate_bps);
        layer_active_.push_back(layer.active);
      }
    }
  }

  queue_ = env_.task_queue_factory().CreateTaskQueue(
      "LkVideoEncoder", webrtc::TaskQueueFactory::Priority::kNormal);

  std::string error;
  webrtc::Event done;
  queue_->PostTask([&] {
    encoder_ = factory_->Create(env_, tagged);
    if (!encoder_) {
      error = "failed to create the encoder";
      done.Set();
      return;
    }
    const webrtc::VideoEncoder::Settings encoder_settings(
        webrtc::VideoEncoder::Capabilities(/*loss_notification=*/false),
        NumberOfCores(), kMaxPayloadSize);
    const int ret = encoder_->InitEncode(&codec_, encoder_settings);
    if (ret != WEBRTC_VIDEO_CODEC_OK) {
      error = "InitEncode failed with error " + std::to_string(ret);
      encoder_->Release();
      encoder_ = nullptr;
      done.Set();
      return;
    }
    encoder_->RegisterEncodeCompleteCallback(callback_.get());
    ApplyRatesOnQueue();
    done.Set();
  });
  done.Wait(webrtc::Event::kForever);
  return error;
}

VideoEncodeResult NativeVideoEncoder::encode(const VideoFrame& frame,
                                             uint32_t rtp_timestamp) const {
  if (const int error = last_error_.exchange(0); error != 0) {
    return VideoEncodeResult{VideoEncodeStatus::EncoderError, error};
  }

  webrtc::VideoFrame input = frame.get();
  auto buffer = input.video_frame_buffer();
  if (!buffer || livekit::EncodedVideoFrameBuffer::FromNative(buffer.get())) {
    return VideoEncodeResult{VideoEncodeStatus::InvalidFrame, 0};
  }

  if (pending_encodes_.fetch_add(1) >= kMaxPendingEncodes) {
    pending_encodes_.fetch_sub(1);
    return VideoEncodeResult{VideoEncodeStatus::Busy, 0};
  }

  queue_->PostTask([this, input = std::move(input), rtp_timestamp] {
    EncodeOnQueue(input, rtp_timestamp);
    pending_encodes_.fetch_sub(1);
  });
  return VideoEncodeResult{VideoEncodeStatus::Queued, 0};
}

void NativeVideoEncoder::EncodeOnQueue(const webrtc::VideoFrame& frame,
                                       uint32_t rtp_timestamp) const {
  if (!encoder_) {
    return;
  }

  {
    webrtc::MutexLock lock(&rates_mutex_);
    bool any_active = false;
    for (size_t i = 0; i < layer_active_.size(); ++i) {
      any_active |= layer_active_[i] && layer_bitrates_bps_[i] > 0;
    }
    if (!any_active) {
      return;
    }
  }

  // Encoders ignore rotation (WebRTC signals it out of band), so apply it to
  // the pixels; then match the configured input size.
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer =
      frame.video_frame_buffer();
  if (frame.rotation() != webrtc::kVideoRotation_0) {
    buffer = webrtc::I420Buffer::Rotate(*buffer->ToI420(), frame.rotation());
  }
  if (buffer->width() != width_ || buffer->height() != height_) {
    buffer = buffer->Scale(width_, height_);
  }

  const webrtc::VideoFrame input = webrtc::VideoFrame::Builder()
                                       .set_video_frame_buffer(buffer)
                                       .set_rtp_timestamp(rtp_timestamp)
                                       .set_timestamp_us(frame.timestamp_us())
                                       .set_rotation(webrtc::kVideoRotation_0)
                                       .build();

  uint32_t requests = keyframe_requests_->exchange(0);
  if (requests != 0 && codec_.codecType == webrtc::kVideoCodecVP9 &&
      num_streams_ > 1) {
    // See request_keyframe.
    requests = (1u << num_streams_) - 1;
  }
  std::vector<webrtc::VideoFrameType> frame_types(
      num_streams_, webrtc::VideoFrameType::kVideoFrameDelta);
  for (int i = 0; i < num_streams_; ++i) {
    if (requests & (1u << i)) {
      frame_types[i] = webrtc::VideoFrameType::kVideoFrameKey;
    }
  }

  callback_->OnEncodeStarted(rtp_timestamp);
  const int32_t ret = encoder_->Encode(input, &frame_types);
  if (ret < 0) {
    RTC_LOG(LS_WARNING) << "Video encoder failed to encode a frame: " << ret;
    last_error_.store(ret);
    // The requested keyframes were not produced; try again next frame.
    keyframe_requests_->fetch_or(requests);
  }
}

void NativeVideoEncoder::request_keyframe(int32_t layer) const {
  // libvpx encodes VP9 simulcast as one SVC stream with independent layers,
  // and only honors keyframe requests for all of them at once.
  if (layer < 0 || (codec_.codecType == webrtc::kVideoCodecVP9 &&
                    num_streams_ > 1)) {
    keyframe_requests_->fetch_or((1u << num_streams_) - 1);
  } else if (layer < num_streams_) {
    keyframe_requests_->fetch_or(1u << layer);
  }
}

void NativeVideoEncoder::set_rates(
    rust::Slice<const uint32_t> layer_bitrates_bps,
    double framerate_fps) const {
  {
    webrtc::MutexLock lock(&rates_mutex_);
    const size_t count =
        std::min(layer_bitrates_bps.size(), layer_bitrates_bps_.size());
    for (size_t i = 0; i < count; ++i) {
      if (layer_bitrates_bps_[i] == 0 && layer_bitrates_bps[i] > 0 &&
          layer_active_[i]) {
        // A layer resumed by the rate allocation has no usable reference
        // frame on the receiving side.
        keyframe_requests_->fetch_or(1u << i);
      }
      layer_bitrates_bps_[i] = layer_bitrates_bps[i];
    }
    if (framerate_fps > 0.0) {
      framerate_fps_ = framerate_fps;
    }
  }
  queue_->PostTask([this] { ApplyRatesOnQueue(); });
}

void NativeVideoEncoder::set_layer_active(uint32_t layer, bool active) const {
  {
    webrtc::MutexLock lock(&rates_mutex_);
    if (layer >= layer_active_.size() || layer_active_[layer] == active) {
      return;
    }
    layer_active_[layer] = active;
  }
  if (active) {
    // A resumed layer has no usable reference frame on the receiving side.
    keyframe_requests_->fetch_or(1u << layer);
  }
  queue_->PostTask([this] { ApplyRatesOnQueue(); });
}

void NativeVideoEncoder::ApplyRatesOnQueue() const {
  if (!encoder_) {
    return;
  }
  webrtc::VideoBitrateAllocation allocation;
  double framerate_fps;
  {
    webrtc::MutexLock lock(&rates_mutex_);
    const float* shares = kTemporalLayerRateShare[num_temporal_layers_ - 1];
    for (size_t i = 0; i < layer_bitrates_bps_.size(); ++i) {
      if (!layer_active_[i] || layer_bitrates_bps_[i] == 0) {
        continue;
      }
      const uint32_t total = layer_bitrates_bps_[i];
      uint32_t allocated = 0;
      for (int tl = 0; tl < num_temporal_layers_; ++tl) {
        const uint32_t cumulative = static_cast<uint32_t>(total * shares[tl]);
        allocation.SetBitrate(i, tl, std::max(cumulative - allocated, 1u));
        allocated = cumulative;
      }
    }
    framerate_fps = framerate_fps_;
  }
  encoder_->SetRates(
      webrtc::VideoEncoder::RateControlParameters(allocation, framerate_fps));
}

VideoCodecInfo NativeVideoEncoder::info() const {
  VideoCodecInfo info;
  webrtc::Event done;
  queue_->PostTask([this, &info, &done] {
    if (encoder_) {
      const auto encoder_info = encoder_->GetEncoderInfo();
      info.implementation_name = encoder_info.implementation_name;
      info.is_hardware_accelerated = encoder_info.is_hardware_accelerated;
    }
    done.Set();
  });
  done.Wait(webrtc::Event::kForever);
  return info;
}

std::unique_ptr<NativeVideoEncoder> create_video_encoder(
    const VideoEncoderSettings& settings,
    rust::Box<VideoEncoderObserver> observer) {
  auto encoder = std::make_unique<NativeVideoEncoder>(std::move(observer));
  if (std::string error = encoder->Initialize(settings); !error.empty()) {
    // Destroys the partially initialized encoder before reporting.
    encoder = nullptr;
    throw BridgeException(error);
  }
  return encoder;
}

// ---------------------------------------------------------------------------
// NativeVideoDecoder
// ---------------------------------------------------------------------------

class NativeVideoDecoder::Callback : public webrtc::DecodedImageCallback {
 public:
  explicit Callback(rust::Box<VideoDecoderObserver> observer)
      : observer_(std::move(observer)) {}

  // Called on the decoder queue right before `rtp_timestamp` is decoded.
  void OnDecodeStarted(uint32_t rtp_timestamp) { timer_.Start(rtp_timestamp); }

  int32_t Decoded(webrtc::VideoFrame& frame) override {
    Deliver(frame, std::nullopt, std::nullopt);
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Decoded(webrtc::VideoFrame& frame, int64_t decode_time_ms) override {
    Deliver(frame, static_cast<int32_t>(decode_time_ms), std::nullopt);
    return WEBRTC_VIDEO_CODEC_OK;
  }

  void Decoded(webrtc::VideoFrame& frame,
               std::optional<int32_t> decode_time_ms,
               std::optional<uint8_t> qp) override {
    Deliver(frame, decode_time_ms, qp);
  }

  // After this returns, no output reaches the observer anymore.
  void Detach() {
    webrtc::MutexLock lock(&mutex_);
    detached_ = true;
  }

 private:
  void Deliver(const webrtc::VideoFrame& frame,
               std::optional<int32_t> decode_time_ms,
               std::optional<uint8_t> qp) {
    DecodedVideoFrameInfo info;
    // Prefer the decoder's own figure, which excludes queueing in hardware.
    info.decode_time_us =
        decode_time_ms && *decode_time_ms >= 0
            ? static_cast<uint32_t>(*decode_time_ms) * 1000
            : timer_.Elapsed(frame.rtp_timestamp());
    info.qp = qp ? static_cast<int32_t>(*qp) : -1;
    webrtc::MutexLock lock(&mutex_);
    if (detached_ || !frame.video_frame_buffer()) {
      return;
    }
    observer_->on_decoded_frame(std::make_unique<VideoFrame>(frame), info);
  }

  webrtc::Mutex mutex_;
  bool detached_ RTC_GUARDED_BY(mutex_) = false;
  rust::Box<VideoDecoderObserver> observer_;
  FrameTimer timer_;
};

namespace {

// Libwebrtc's own (software) decoder of `codec`, or nullptr.
std::unique_ptr<webrtc::VideoDecoder> CreateSoftwareDecoder(
    const webrtc::Environment& env,
    EncodedVideoCodec codec) {
  switch (codec) {
    case EncodedVideoCodec::VP8:
      return webrtc::CreateVp8Decoder(env);
    case EncodedVideoCodec::VP9:
      return webrtc::VP9Decoder::Create();
    case EncodedVideoCodec::H264:
      return webrtc::H264Decoder::IsSupported() ? webrtc::H264Decoder::Create()
                                                : nullptr;
    case EncodedVideoCodec::AV1:
#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
      return webrtc::CreateDav1dDecoder();
#else
      return nullptr;
#endif
    default:
      return nullptr;
  }
}

}  // namespace

NativeVideoDecoder::NativeVideoDecoder(rust::Box<VideoDecoderObserver> observer)
    : env_(webrtc::CreateEnvironment()),
      callback_(std::make_unique<Callback>(std::move(observer))) {
  g_live_video_codecs.fetch_add(1);
}

NativeVideoDecoder::~NativeVideoDecoder() {
  callback_->Detach();
  if (queue_) {
    webrtc::Event done;
    queue_->PostTask([this, &done] {
      if (decoder_) {
        decoder_->Release();
        decoder_ = nullptr;
      }
      done.Set();
    });
    done.Wait(webrtc::Event::kForever);
    queue_ = nullptr;
  }
  g_live_video_codecs.fetch_sub(1);
}

std::string NativeVideoDecoder::Initialize(EncodedVideoCodec codec,
                                           bool prefer_software) {
  if (!IsCodec(codec)) {
    return "unknown codec";
  }
  factory_ = SharedFactory<VideoDecoderFactory>();

  // Codec parameters (H.264 profile, VP9 profile, ...) do not restrict what
  // the decoders accept, so the first advertised format of the codec will do.
  // Formats the factory can create without advertising them (e.g. the
  // internal H.264 decoder) are tried by name.
  webrtc::SdpVideoFormat format(CodecName(codec));
  for (const auto& supported : factory_->GetSupportedFormats()) {
    if (MatchesCodecName(supported.name, codec)) {
      format = supported;
      break;
    }
  }

  queue_ = env_.task_queue_factory().CreateTaskQueue(
      "LkVideoDecoder", webrtc::TaskQueueFactory::Priority::kNormal);

  std::string error;
  webrtc::Event done;
  queue_->PostTask([&] {
    decoder_ = prefer_software ? CreateSoftwareDecoder(env_, codec)
                               : factory_->Create(env_, format);
    if (!decoder_) {
      error = std::string("no ") + CodecName(codec) + " decoder is available";
      done.Set();
      return;
    }
    webrtc::VideoDecoder::Settings settings;
    settings.set_codec_type(ToCodecType(codec));
    settings.set_number_of_cores(NumberOfCores());
    if (!decoder_->Configure(settings)) {
      error = std::string("failed to configure the ") + CodecName(codec) +
              " decoder";
      decoder_->Release();
      decoder_ = nullptr;
      done.Set();
      return;
    }
    decoder_->RegisterDecodeCompleteCallback(callback_.get());
    done.Set();
  });
  done.Wait(webrtc::Event::kForever);
  return error;
}

VideoDecodeStatus NativeVideoDecoder::decode(rust::Slice<const uint8_t> data,
                                             bool is_keyframe,
                                             uint32_t rtp_timestamp,
                                             uint32_t width,
                                             uint32_t height) const {
  if (data.empty()) {
    return VideoDecodeStatus::InvalidInput;
  }

  uint64_t generation;
  if (is_keyframe) {
    generation = keyframe_generation_.fetch_add(1) + 1;
    needs_keyframe_.store(false);
  } else {
    if (needs_keyframe_.load()) {
      return VideoDecodeStatus::KeyframeRequired;
    }
    generation = keyframe_generation_.load();
  }

  if (pending_decodes_.fetch_add(1) >= kMaxPendingDecodes) {
    pending_decodes_.fetch_sub(1);
    // Later delta frames may reference the dropped one.
    needs_keyframe_.store(true);
    return VideoDecodeStatus::KeyframeRequired;
  }

  webrtc::EncodedImage image;
  image.SetEncodedData(
      webrtc::EncodedImageBuffer::Create(data.data(), data.size()));
  image.SetRtpTimestamp(rtp_timestamp);
  image.SetFrameType(is_keyframe ? webrtc::VideoFrameType::kVideoFrameKey
                                 : webrtc::VideoFrameType::kVideoFrameDelta);
  image._encodedWidth = width;
  image._encodedHeight = height;

  queue_->PostTask([this, image = std::move(image), generation] {
    DecodeOnQueue(image, generation);
    pending_decodes_.fetch_sub(1);
  });
  return VideoDecodeStatus::Queued;
}

void NativeVideoDecoder::DecodeOnQueue(const webrtc::EncodedImage& image,
                                       uint64_t generation) const {
  if (!decoder_) {
    return;
  }
  const bool is_keyframe = image.IsKey();
  if (queue_needs_keyframe_ && !is_keyframe) {
    return;
  }
  queue_needs_keyframe_ = false;

  callback_->OnDecodeStarted(image.RtpTimestamp());
  const int32_t ret =
      decoder_->Decode(image, /*missing_frames=*/false, /*render_time_ms=*/0);
  if (ret == WEBRTC_VIDEO_CODEC_OK) {
    return;
  }
  if (ret < 0) {
    decode_errors_.fetch_add(1);
    RTC_LOG(LS_WARNING) << "Video decoder failed to decode a "
                        << (is_keyframe ? "keyframe" : "delta frame") << ": "
                        << ret;
    queue_needs_keyframe_ = true;
  }
  if (ret < 0 || ret == WEBRTC_VIDEO_CODEC_OK_REQUEST_KEYFRAME) {
    // Unless the caller already submitted a newer keyframe, reject delta
    // frames until it does.
    if (keyframe_generation_.load() == generation) {
      needs_keyframe_.store(true);
    }
  }
}

VideoCodecInfo NativeVideoDecoder::info() const {
  VideoCodecInfo info;
  webrtc::Event done;
  queue_->PostTask([this, &info, &done] {
    if (decoder_) {
      const auto decoder_info = decoder_->GetDecoderInfo();
      info.implementation_name = decoder_info.implementation_name;
      info.is_hardware_accelerated = decoder_info.is_hardware_accelerated;
    }
    done.Set();
  });
  done.Wait(webrtc::Event::kForever);
  return info;
}

std::unique_ptr<NativeVideoDecoder> create_video_decoder(
    EncodedVideoCodec codec,
    bool prefer_software,
    rust::Box<VideoDecoderObserver> observer) {
  auto decoder = std::make_unique<NativeVideoDecoder>(std::move(observer));
  if (std::string error = decoder->Initialize(codec, prefer_software);
      !error.empty()) {
    decoder = nullptr;
    throw BridgeException(error);
  }
  return decoder;
}

// ---------------------------------------------------------------------------
// Pre-encoded buffers
// ---------------------------------------------------------------------------

EncodedBufferInfo encoded_buffer_info(const VideoFrameBuffer& buffer) {
  EncodedBufferInfo info{};
  auto* encoded = livekit::EncodedVideoFrameBuffer::FromNative(buffer.get().get());
  if (!encoded) {
    return info;
  }
  info.is_encoded = true;
  switch (encoded->codec()) {
    case livekit::EncodedVideoCodec::kH264:
      info.codec = EncodedVideoCodec::H264;
      break;
    case livekit::EncodedVideoCodec::kH265:
      info.codec = EncodedVideoCodec::H265;
      break;
    case livekit::EncodedVideoCodec::kVP8:
      info.codec = EncodedVideoCodec::VP8;
      break;
    case livekit::EncodedVideoCodec::kVP9:
      info.codec = EncodedVideoCodec::VP9;
      break;
    case livekit::EncodedVideoCodec::kAV1:
      info.codec = EncodedVideoCodec::AV1;
      break;
  }
  info.is_keyframe = encoded->frame_type() == livekit::EncodedFrameType::kKey;
  info.width = static_cast<uint32_t>(encoded->width());
  info.height = static_cast<uint32_t>(encoded->height());
  return info;
}

rust::Slice<const uint8_t> encoded_buffer_payload(
    const VideoFrameBuffer& buffer) {
  auto* encoded = livekit::EncodedVideoFrameBuffer::FromNative(buffer.get().get());
  if (!encoded || encoded->payload_size() == 0) {
    return rust::Slice<const uint8_t>();
  }
  return rust::Slice<const uint8_t>(encoded->payload_data(),
                                    encoded->payload_size());
}

rust::Vec<uint8_t> rewrite_h264_keyframe(rust::Slice<const uint8_t> data) {
  const webrtc::Buffer rewritten =
      webrtc::SpsVuiRewriter::ParseOutgoingBitstreamAndRewrite(
          std::span<const uint8_t>(data.data(), data.size()), nullptr);
  rust::Vec<uint8_t> out;
  out.reserve(rewritten.size());
  for (uint8_t byte : rewritten) {
    out.push_back(byte);
  }
  return out;
}

int64_t live_video_codec_count() {
  return g_live_video_codecs.load();
}

}  // namespace livekit_ffi
