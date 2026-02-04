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

#include "livekit_rtc/passthrough_encoder.h"

#include <cstdio>
#include <cstring>

#include "livekit_rtc/encoded_video_source.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"

namespace livekit_ffi {

PassthroughVideoEncoder::PassthroughVideoEncoder(
    webrtc::VideoCodecType codec_type)
    : codec_type_(codec_type) {}

PassthroughVideoEncoder::~PassthroughVideoEncoder() {
  Release();
}

void PassthroughVideoEncoder::SetFecControllerOverride(
    webrtc::FecControllerOverride* fec_controller_override) {
  // Not used for passthrough encoding
}

int PassthroughVideoEncoder::InitEncode(const webrtc::VideoCodec* codec_settings,
                                        const Settings& settings) {
  webrtc::MutexLock lock(&mutex_);
  if (!codec_settings) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  configured_width_ = codec_settings->width;
  configured_height_ = codec_settings->height;
  max_framerate_ = codec_settings->maxFramerate;
  initialized_ = true;

  RTC_LOG(LS_INFO) << "PassthroughVideoEncoder initialized: "
                   << configured_width_ << "x" << configured_height_
                   << " @ " << max_framerate_ << " fps";

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::RegisterEncodeCompleteCallback(
    webrtc::EncodedImageCallback* callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Release() {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
  initialized_ = false;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoEncoder::Encode(
    const webrtc::VideoFrame& frame,
    const std::vector<webrtc::VideoFrameType>* frame_types) {
  webrtc::MutexLock lock(&mutex_);

  if (!initialized_ || !callback_) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  // Use the frame ID to find the right provider
  uint16_t frame_id = frame.id();
  auto& registry = EncodedVideoSourceRegistry::GetInstance();

  EncodedFrameProvider* provider = registry.GetProvider(frame_id);
  if (!provider) {
    // This frame is not from an encoded video source - shouldn't happen
    // if the encoder factory is working correctly
    RTC_LOG(LS_WARNING) << "PassthroughVideoEncoder: No provider for frame ID "
                        << frame_id;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  // Check if a keyframe was requested
  bool keyframe_requested = false;
  if (frame_types) {
    for (const auto& frame_type : *frame_types) {
      if (frame_type == webrtc::VideoFrameType::kVideoFrameKey) {
        keyframe_requested = true;
        break;
      }
    }
  }

  if (keyframe_requested) {
    provider->RequestKeyFrame();
  }

  // Get the next encoded frame from the provider
  auto encoded_frame = provider->GetNextEncodedFrame();
  if (!encoded_frame) {
    // No frame available - this shouldn't happen in normal operation
    // since we only get Encode calls when we push frames
    fprintf(stderr, "[PassthroughEncoder] WARNING: No encoded frame available!\n");
    return WEBRTC_VIDEO_CODEC_OK;
  }

  // Build the EncodedImage
  webrtc::EncodedImage encoded_image;
  encoded_image.SetEncodedData(encoded_frame->data);
  encoded_image._encodedWidth = encoded_frame->width;
  encoded_image._encodedHeight = encoded_frame->height;

  // Always use the pre-encoded frame's RTP timestamp
  // Note: 0 is a valid starting timestamp, so we always use the provided value
  uint32_t rtp_timestamp = encoded_frame->rtp_timestamp;
  encoded_image.SetRtpTimestamp(rtp_timestamp);

  // Always use the pre-encoded frame's capture time (convert us to ms)
  int64_t capture_time_ms = encoded_frame->capture_time_us / 1000;
  encoded_image.capture_time_ms_ = capture_time_ms;

  encoded_image._frameType = encoded_frame->is_keyframe
                                 ? webrtc::VideoFrameType::kVideoFrameKey
                                 : webrtc::VideoFrameType::kVideoFrameDelta;
  encoded_image.rotation_ = frame.rotation();
  encoded_image.content_type_ = webrtc::VideoContentType::UNSPECIFIED;
  encoded_image.timing_.flags = webrtc::VideoSendTiming::kInvalid;

  // Create codec-specific info (memset to ensure proper initialization)
  webrtc::CodecSpecificInfo codec_info;
  memset(&codec_info, 0, sizeof(codec_info));
  codec_info.codecType = codec_type_;

  if (codec_type_ == webrtc::kVideoCodecH264) {
    codec_info.codecSpecific.H264.packetization_mode =
        webrtc::H264PacketizationMode::NonInterleaved;
    codec_info.codecSpecific.H264.temporal_idx = 0;  // No temporal layers
    codec_info.codecSpecific.H264.idr_frame = encoded_frame->is_keyframe;
    codec_info.codecSpecific.H264.base_layer_sync = false;
  }

  // Log only keyframes to reduce overhead
  static int frame_count = 0;
  frame_count++;
  if (encoded_frame->is_keyframe) {
    fprintf(stderr, "[PassthroughEncoder] Keyframe %d: size=%zu, rtp_ts=%u\n",
            frame_count, encoded_frame->data->size(), rtp_timestamp);
  }

  // Send the encoded frame
  webrtc::EncodedImageCallback::Result result =
      callback_->OnEncodedImage(encoded_image, &codec_info);

  if (result.error != webrtc::EncodedImageCallback::Result::OK) {
    fprintf(stderr, "[PassthroughEncoder] OnEncodedImage FAILED: error=%d\n",
            static_cast<int>(result.error));
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

void PassthroughVideoEncoder::SetRates(const RateControlParameters& parameters) {
  webrtc::MutexLock lock(&mutex_);
  target_bitrate_bps_ = parameters.bitrate.get_sum_bps();
  // Passthrough encoder doesn't do rate control, but we store the value
  // for GetEncoderInfo
}

void PassthroughVideoEncoder::OnPacketLossRateUpdate(float packet_loss_rate) {
  // Passthrough encoder doesn't adapt to packet loss
}

void PassthroughVideoEncoder::OnRttUpdate(int64_t rtt_ms) {
  // Passthrough encoder doesn't adapt to RTT
}

void PassthroughVideoEncoder::OnLossNotification(
    const LossNotification& loss_notification) {
  // We cannot request keyframes here since we don't know which provider
  // to use without a frame. The encoder will request keyframes on the
  // next Encode() call if needed.
}

webrtc::VideoEncoder::EncoderInfo PassthroughVideoEncoder::GetEncoderInfo()
    const {
  webrtc::MutexLock lock(&mutex_);

  EncoderInfo info;
  info.implementation_name = "PassthroughEncoder";
  info.supports_native_handle = false;
  info.has_trusted_rate_controller = true;  // Trust our rate control, don't drop frames
  info.is_hardware_accelerated = false;
  info.is_qp_trusted = false;
  info.scaling_settings = ScalingSettings::kOff;

  // We support any resolution for passthrough
  info.resolution_bitrate_limits = {};

  return info;
}

}  // namespace livekit_ffi
