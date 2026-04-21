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

#include "livekit/passthrough_video_encoder.h"

#include <algorithm>
#include <utility>

#include "api/video/encoded_image.h"
#include "api/video/video_codec_type.h"
#include "api/video/video_frame_type.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"

namespace livekit_ffi {

namespace {

webrtc::VideoCodecType ToWebrtcCodec(EncodedVideoCodecType codec) {
  switch (codec) {
    case EncodedVideoCodecType::H264:
      return webrtc::kVideoCodecH264;
    case EncodedVideoCodecType::H265:
      return webrtc::kVideoCodecH265;
    case EncodedVideoCodecType::Vp8:
      return webrtc::kVideoCodecVP8;
    case EncodedVideoCodecType::Vp9:
      return webrtc::kVideoCodecVP9;
    case EncodedVideoCodecType::Av1:
      return webrtc::kVideoCodecAV1;
    default:
      return webrtc::kVideoCodecGeneric;
  }
}

bool FrameTypesRequestKeyframe(
    const std::vector<webrtc::VideoFrameType>* frame_types) {
  if (!frame_types) return false;
  return std::any_of(frame_types->begin(), frame_types->end(),
                     [](webrtc::VideoFrameType t) {
                       return t == webrtc::VideoFrameType::kVideoFrameKey;
                     });
}

}  // namespace

// ---------- PassthroughVideoEncoder ----------

PassthroughVideoEncoder::PassthroughVideoEncoder(
    webrtc::scoped_refptr<EncodedVideoTrackSource::InternalSource> source)
    : source_(std::move(source)),
      codec_(source_ ? source_->codec() : EncodedVideoCodecType::H264) {
  RTC_DCHECK(source_);
}

PassthroughVideoEncoder::~PassthroughVideoEncoder() = default;

int PassthroughVideoEncoder::InitEncode(const webrtc::VideoCodec* codec_settings,
                                        const Settings& settings) {
  if (codec_settings) {
    codec_settings_ = *codec_settings;
  }
  initialized_ = true;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::RegisterEncodeCompleteCallback(
    webrtc::EncodedImageCallback* callback) {
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Release() {
  callback_ = nullptr;
  initialized_ = false;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Encode(
    const webrtc::VideoFrame& frame,
    const std::vector<webrtc::VideoFrameType>* frame_types) {
  if (!initialized_ || !callback_) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  if (FrameTypesRequestKeyframe(frame_types)) {
    source_->notify_keyframe_requested();
  }

  EncodedVideoTrackSource::InternalSource::DequeuedFrame enc;
  if (!src->get()->pop_encoded_frame(enc)) {
    // No bytes queued for this tick; treat as a dropped frame so WebRTC's
    // pacing accounting is correct.
    callback_->OnDroppedFrame(
        webrtc::EncodedImageCallback::DropReason::kDroppedByEncoder);
    return WEBRTC_VIDEO_CODEC_OK;
  }

  webrtc::EncodedImage image;
  image.SetEncodedData(webrtc::EncodedImageBuffer::Create(
      enc.data.data(), enc.data.size()));
  image.SetFrameType(enc.is_keyframe ? webrtc::VideoFrameType::kVideoFrameKey
                                     : webrtc::VideoFrameType::kVideoFrameDelta);
  image.SetRtpTimestamp(frame.rtp_timestamp());
  image.capture_time_ms_ = enc.capture_time_us != 0
                               ? enc.capture_time_us / 1000
                               : frame.render_time_ms();
  image._encodedWidth = enc.width;
  image._encodedHeight = enc.height;
  image.rotation_ = frame.rotation();

  webrtc::CodecSpecificInfo info{};
  info.codecType = ToWebrtcCodec(codec_);
  info.end_of_picture = true;

  auto result = callback_->OnEncodedImage(image, &info);
  if (result.error != webrtc::EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_WARNING)
        << "PassthroughVideoEncoder OnEncodedImage failed; send_failed="
        << (result.error ==
            webrtc::EncodedImageCallback::Result::ERROR_SEND_FAILED);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

void PassthroughVideoEncoder::SetRates(const RateControlParameters& parameters) {
  const uint32_t target_bps = parameters.target_bitrate.get_sum_bps();
  const double framerate = parameters.framerate_fps;
  source_->notify_target_bitrate(target_bps, framerate);
}

webrtc::VideoEncoder::EncoderInfo PassthroughVideoEncoder::GetEncoderInfo()
    const {
  EncoderInfo info;
  info.implementation_name = "LiveKitPassthrough";
  info.is_hardware_accelerated = false;
  info.supports_native_handle = false;
  info.has_trusted_rate_controller = true;
  info.supports_simulcast = false;
  info.requested_resolution_alignment = 1;
  info.apply_alignment_to_all_simulcast_layers = false;
  return info;
}

// ---------- LazyVideoEncoder ----------

LazyVideoEncoder::LazyVideoEncoder(webrtc::SdpVideoFormat format,
                                   RealEncoderBuilder real_encoder_builder)
    : format_(std::move(format)),
      real_encoder_builder_(std::move(real_encoder_builder)) {}

LazyVideoEncoder::~LazyVideoEncoder() = default;

int LazyVideoEncoder::InitEncode(const webrtc::VideoCodec* codec_settings,
                                 const Settings& settings) {
  if (codec_settings) {
    pending_codec_settings_ = *codec_settings;
  }
  pending_settings_ = settings;
  has_pending_init_ = true;

  // If we already built an inner (e.g. re-init), forward immediately.
  if (inner_) {
    return inner_->InitEncode(codec_settings, settings);
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t LazyVideoEncoder::RegisterEncodeCompleteCallback(
    webrtc::EncodedImageCallback* callback) {
  callback_ = callback;
  if (inner_) {
    return inner_->RegisterEncodeCompleteCallback(callback);
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t LazyVideoEncoder::Release() {
  int32_t rc = WEBRTC_VIDEO_CODEC_OK;
  if (inner_) {
    rc = inner_->Release();
  }
  inner_.reset();
  has_pending_init_ = false;
  pending_rates_.reset();
  pending_loss_rate_.reset();
  pending_rtt_ms_.reset();
  callback_ = nullptr;
  return rc;
}

bool LazyVideoEncoder::BuildInner(uint16_t frame_id) {
  EncodedVideoTrackSource* src =
      EncodedSourceRegistry::instance().lookup(frame_id);

  if (src != nullptr) {
    inner_ = std::make_unique<PassthroughVideoEncoder>(src->get());
    is_passthrough_ = true;
    RTC_LOG(LS_INFO)
        << "LazyVideoEncoder: using PassthroughVideoEncoder for source id="
        << frame_id << " codec=" << static_cast<int>(src->codec())
        << " sdp=" << format_.name;
  } else {
    inner_ = real_encoder_builder_ ? real_encoder_builder_() : nullptr;
    is_passthrough_ = false;
    if (!inner_) {
      RTC_LOG(LS_ERROR)
          << "LazyVideoEncoder: real_encoder_builder returned null for "
          << format_.name;
      return false;
    }
  }

  if (callback_) {
    inner_->RegisterEncodeCompleteCallback(callback_);
  }
  if (has_pending_init_) {
    int rc = inner_->InitEncode(&pending_codec_settings_, pending_settings_);
    if (rc != WEBRTC_VIDEO_CODEC_OK) {
      RTC_LOG(LS_ERROR) << "LazyVideoEncoder: inner InitEncode failed rc="
                        << rc;
      return false;
    }
  }
  if (pending_rates_) {
    inner_->SetRates(*pending_rates_);
    pending_rates_.reset();
  }
  if (pending_loss_rate_) {
    inner_->OnPacketLossRateUpdate(*pending_loss_rate_);
    pending_loss_rate_.reset();
  }
  if (pending_rtt_ms_) {
    inner_->OnRttUpdate(*pending_rtt_ms_);
    pending_rtt_ms_.reset();
  }
  return true;
}

int32_t LazyVideoEncoder::Encode(
    const webrtc::VideoFrame& frame,
    const std::vector<webrtc::VideoFrameType>* frame_types) {
  if (!inner_) {
    if (!BuildInner(frame.id())) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }
  return inner_->Encode(frame, frame_types);
}

void LazyVideoEncoder::SetRates(const RateControlParameters& parameters) {
  if (inner_) {
    inner_->SetRates(parameters);
  } else {
    pending_rates_ = parameters;
  }
}

void LazyVideoEncoder::OnPacketLossRateUpdate(float packet_loss_rate) {
  if (inner_) {
    inner_->OnPacketLossRateUpdate(packet_loss_rate);
  } else {
    pending_loss_rate_ = packet_loss_rate;
  }
}

void LazyVideoEncoder::OnRttUpdate(int64_t rtt_ms) {
  if (inner_) {
    inner_->OnRttUpdate(rtt_ms);
  } else {
    pending_rtt_ms_ = rtt_ms;
  }
}

void LazyVideoEncoder::OnLossNotification(
    const LossNotification& loss_notification) {
  if (inner_) {
    inner_->OnLossNotification(loss_notification);
  }
}

webrtc::VideoEncoder::EncoderInfo LazyVideoEncoder::GetEncoderInfo() const {
  if (inner_) {
    return inner_->GetEncoderInfo();
  }
  EncoderInfo info;
  info.implementation_name = "LiveKitLazy";
  info.is_hardware_accelerated = false;
  info.supports_native_handle = false;
  info.requested_resolution_alignment = 1;
  info.apply_alignment_to_all_simulcast_layers = false;
  return info;
}

}  // namespace livekit_ffi
