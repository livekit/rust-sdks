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

#include "h265_decoder_impl.h"

#include <optional>

#include "api/video/i420_buffer.h"
#include "api/video/video_codec_type.h"
#include "api/video/video_frame.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "third_party/libyuv/include/libyuv/convert.h"

namespace webrtc {

JetsonH265DecoderImpl::JetsonH265DecoderImpl()
    : decoder_(livekit::JetsonCodec::kH265), buffer_pool_(false) {}

JetsonH265DecoderImpl::~JetsonH265DecoderImpl() {
  Release();
}

VideoDecoder::DecoderInfo JetsonH265DecoderImpl::GetDecoderInfo() const {
  DecoderInfo info;
  info.implementation_name = "Jetson MMAPI H265 Decoder";
  info.is_hardware_accelerated = true;
  return info;
}

bool JetsonH265DecoderImpl::Configure(const Settings& settings) {
  if (settings.codec_type() != kVideoCodecH265) {
    RTC_LOG(LS_ERROR) << "Jetson H265 decoder configured for another codec.";
    return false;
  }
  return StartDecoder();
}

bool JetsonH265DecoderImpl::StartDecoder() {
  awaiting_keyframe_ = true;
  // The capture thread is joined in Release() and the destructor, before
  // `this` goes away, so it never outlives the object it calls into.
  return decoder_.Initialize(
      [this](const livekit::JetsonDecodedFrame& frame) { OnFrame(frame); });
}

int32_t JetsonH265DecoderImpl::RegisterDecodeCompleteCallback(
    DecodedImageCallback* callback) {
  decoded_complete_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH265DecoderImpl::Release() {
  decoder_.Destroy();
  buffer_pool_.Release();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH265DecoderImpl::Decode(const EncodedImage& input_image,
                                      bool /*missing_frames*/,
                                      int64_t /*render_time_ms*/) {
  if (!decoder_.IsInitialized() || decoded_complete_callback_ == nullptr) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!input_image.data() || input_image.size() == 0) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  if (decoder_.HasFailed()) {
    RTC_LOG(LS_WARNING) << "Jetson H265 decoder failed; restarting it.";
    if (!StartDecoder()) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }
  // An error makes WebRTC request a keyframe from the sender.
  if (awaiting_keyframe_ && !input_image.IsKey()) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  awaiting_keyframe_ = false;

  if (!decoder_.Decode(input_image.data(), input_image.size(),
                       input_image.RtpTimestamp())) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

void JetsonH265DecoderImpl::OnFrame(const livekit::JetsonDecodedFrame& frame) {
  DecodedImageCallback* callback = decoded_complete_callback_;
  if (!callback) {
    return;
  }

  scoped_refptr<I420Buffer> buffer =
      buffer_pool_.CreateI420Buffer(frame.width, frame.height);
  if (!buffer) {
    RTC_LOG(LS_WARNING) << "Jetson H265 decoder buffer pool exhausted; "
                           "dropping a picture.";
    return;
  }
  libyuv::I420Copy(frame.data_y, frame.stride_y, frame.data_u, frame.stride_u,
                   frame.data_v, frame.stride_v, buffer->MutableDataY(),
                   buffer->StrideY(), buffer->MutableDataU(),
                   buffer->StrideU(), buffer->MutableDataV(),
                   buffer->StrideV(), frame.width, frame.height);

  VideoFrame decoded = VideoFrame::Builder()
                           .set_video_frame_buffer(buffer)
                           .set_rtp_timestamp(frame.rtp_timestamp)
                           .build();
  callback->Decoded(decoded, std::nullopt, std::nullopt);
}

}  // namespace webrtc
