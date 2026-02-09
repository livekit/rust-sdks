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

#include "livekit/passthrough_video_encoder.h"

#include "api/video/encoded_image.h"
#include "api/video/video_codec_type.h"
#include "api/video_codecs/video_codec.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

// ---------- PassthroughVideoEncoder ----------

PassthroughVideoEncoder::PassthroughVideoEncoder(
    std::shared_ptr<EncodedVideoTrackSource> source)
    : source_(std::move(source)) {}

int32_t PassthroughVideoEncoder::InitEncode(
    const webrtc::VideoCodec* codec_settings,
    const webrtc::VideoEncoder::Settings& settings) {
  if (!codec_settings) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  codec_ = *codec_settings;
  sending_ = true;

  // Derive our simulcast index from the codec settings.
  // When used inside a SimulcastEncoderAdapter, the adapter sets
  // numberOfSimulcastStreams=1 and configures the single stream's
  // dimensions.  We match by resolution to find our layer index.
  simulcast_index_ = 0;
  if (codec_settings->numberOfSimulcastStreams > 0) {
    // SimulcastEncoderAdapter sets this encoder's target dimensions;
    // find which simulcast stream index matches.
    for (int i = 0; i < codec_settings->numberOfSimulcastStreams; i++) {
      const auto& stream = codec_settings->simulcastStream[i];
      if (stream.width == codec_settings->width &&
          stream.height == codec_settings->height) {
        simulcast_index_ = static_cast<uint32_t>(i);
        break;
      }
    }
  }

  RTC_LOG(LS_INFO) << "PassthroughVideoEncoder::InitEncode "
                   << codec_.width << "x" << codec_.height
                   << " simulcast_index=" << simulcast_index_;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::RegisterEncodeCompleteCallback(
    webrtc::EncodedImageCallback* callback) {
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Release() {
  callback_ = nullptr;
  sending_ = false;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Encode(
    const webrtc::VideoFrame& frame,
    const std::vector<webrtc::VideoFrameType>* frame_types) {
  if (!callback_) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!sending_) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  // Check if WebRTC is requesting a keyframe
  bool keyframe_requested = false;
  if (frame_types) {
    for (auto ft : *frame_types) {
      if (ft == webrtc::VideoFrameType::kVideoFrameKey) {
        keyframe_requested = true;
        break;
      }
    }
  }

  // Signal the keyframe request to the Rust side
  if (keyframe_requested) {
    source_->source_->request_keyframe();
    // Also invoke the Rust callback if set
    webrtc::MutexLock lock(&source_->cb_mutex_);
    if (source_->keyframe_observer_) {
      (*source_->keyframe_observer_)->on_keyframe_request();
    }
  }

  // Pull the queued encoded frame for our simulcast layer
  auto encoded = source_->source_->dequeue_frame(simulcast_index_);
  if (!encoded.has_value()) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  const auto& data = encoded.value();

  // Build the EncodedImage
  webrtc::EncodedImage encoded_image;
  encoded_image.SetEncodedData(
      webrtc::EncodedImageBuffer::Create(data.data.data(), data.data.size()));
  encoded_image.set_size(data.data.size());
  encoded_image._encodedWidth = data.width;
  encoded_image._encodedHeight = data.height;

  // RTP timestamp: if the caller provided one use it, otherwise inherit the
  // timestamp that the WebRTC pipeline assigned to the dummy VideoFrame.
  // This is critical — without a proper, monotonically increasing RTP
  // timestamp the remote jitter buffer cannot order frames and will not
  // render anything.
  if (data.rtp_timestamp != 0) {
    encoded_image.SetRtpTimestamp(data.rtp_timestamp);
  } else {
    encoded_image.SetRtpTimestamp(frame.rtp_timestamp());
  }

  // Timing fields — mirror what hardware encoders (NVIDIA, VAAPI) set from
  // the incoming VideoFrame so the RTP sender and remote jitter buffer see
  // consistent, monotonically-increasing times.
  encoded_image.ntp_time_ms_ = frame.ntp_time_ms();
  encoded_image.capture_time_ms_ = frame.render_time_ms();
  encoded_image.rotation_ = webrtc::kVideoRotation_0;
  encoded_image.content_type_ = webrtc::VideoContentType::UNSPECIFIED;
  encoded_image.timing_.flags = webrtc::VideoSendTiming::kInvalid;
  encoded_image._frameType = data.is_keyframe
                                 ? webrtc::VideoFrameType::kVideoFrameKey
                                 : webrtc::VideoFrameType::kVideoFrameDelta;
  encoded_image.SetSimulcastIndex(simulcast_index_);

  // Build codec-specific info
  webrtc::CodecSpecificInfo codec_info;
  switch (source_->codec_type()) {
    case VideoCodecType::H264:
      codec_info.codecType = webrtc::kVideoCodecH264;
      codec_info.codecSpecific.H264.packetization_mode =
          webrtc::H264PacketizationMode::NonInterleaved;
      break;
    case VideoCodecType::VP8:
      codec_info.codecType = webrtc::kVideoCodecVP8;
      break;
    case VideoCodecType::VP9:
      codec_info.codecType = webrtc::kVideoCodecVP9;
      break;
    case VideoCodecType::AV1:
      codec_info.codecType = webrtc::kVideoCodecAV1;
      break;
    case VideoCodecType::H265:
      codec_info.codecType = webrtc::kVideoCodecH265;
      break;
    default:
      codec_info.codecType = webrtc::kVideoCodecH264;
      break;
  }

  auto result = callback_->OnEncodedImage(encoded_image, &codec_info);
  if (result.error != webrtc::EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "PassthroughVideoEncoder: OnEncodedImage failed: "
                      << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

void PassthroughVideoEncoder::SetRates(
    const RateControlParameters& parameters) {
  // Passthrough encoder doesn't control bitrate -- that's handled externally.
  if (parameters.bitrate.get_sum_bps() == 0) {
    sending_ = false;
  } else {
    sending_ = true;
  }
}

webrtc::VideoEncoder::EncoderInfo
PassthroughVideoEncoder::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "PassthroughVideoEncoder";
  info.scaling_settings = webrtc::VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = false;
  info.supports_simulcast = true;
  info.preferred_pixel_formats = {webrtc::VideoFrameBuffer::Type::kI420};
  return info;
}

// ---------- PassthroughVideoEncoderFactory ----------

PassthroughVideoEncoderFactory::PassthroughVideoEncoderFactory(
    std::shared_ptr<EncodedVideoTrackSource> source,
    const webrtc::SdpVideoFormat& format)
    : source_(std::move(source)), format_(format) {}

std::vector<webrtc::SdpVideoFormat>
PassthroughVideoEncoderFactory::GetSupportedFormats() const {
  return {format_};
}

std::unique_ptr<webrtc::VideoEncoder>
PassthroughVideoEncoderFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  return std::make_unique<PassthroughVideoEncoder>(source_);
}

// ---------- EncodedSourceRegistry ----------

EncodedSourceRegistry& EncodedSourceRegistry::instance() {
  static EncodedSourceRegistry registry;
  return registry;
}

void EncodedSourceRegistry::register_source(
    const webrtc::VideoTrackSourceInterface* key,
    std::shared_ptr<EncodedVideoTrackSource> source) {
  std::lock_guard<std::mutex> lock(mutex_);
  sources_[key] = std::move(source);
}

void EncodedSourceRegistry::unregister_source(
    const webrtc::VideoTrackSourceInterface* key) {
  std::lock_guard<std::mutex> lock(mutex_);
  sources_.erase(key);
}

std::shared_ptr<EncodedVideoTrackSource> EncodedSourceRegistry::find(
    const webrtc::VideoTrackSourceInterface* key) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = sources_.find(key);
  if (it != sources_.end()) {
    return it->second;
  }
  return nullptr;
}

static std::string codec_type_to_sdp_name(VideoCodecType codec) {
  switch (codec) {
    case VideoCodecType::VP8:
      return "VP8";
    case VideoCodecType::VP9:
      return "VP9";
    case VideoCodecType::AV1:
      return "AV1";
    case VideoCodecType::H264:
      return "H264";
    case VideoCodecType::H265:
      return "H265";
    default:
      return "";
  }
}

std::shared_ptr<EncodedVideoTrackSource> EncodedSourceRegistry::find_by_codec_name(
    const std::string& codec_name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  for (const auto& [key, source] : sources_) {
    if (codec_type_to_sdp_name(source->codec_type()) == codec_name) {
      return source;
    }
  }
  return nullptr;
}

}  // namespace livekit_ffi
