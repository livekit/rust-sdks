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

#include "h264_encoder_impl.h"

#include <algorithm>
#include <limits>
#include <string>

#include "api/video/video_codec_constants.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "modules/video_coding/utility/simulcast_utility.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "system_wrappers/include/metrics.h"

namespace webrtc {

// Histogram event codes -- values must not be changed (persisted metrics).
enum V4L2H264EncoderImplEvent {
  kV4L2H264EncoderEventInit = 0,
  kV4L2H264EncoderEventError = 1,
  kV4L2H264EncoderEventMax = 16,
};

// ---------------------------------------------------------------------------
// Construction / destruction
// ---------------------------------------------------------------------------

V4L2H264EncoderImpl::V4L2H264EncoderImpl(const webrtc::Environment& env,
                                           const SdpVideoFormat& format)
    : env_(env),
      encoder_(std::make_unique<livekit_ffi::V4l2H264EncoderWrapper>()),
      packetization_mode_(H264EncoderSettings::Parse(format).packetization_mode),
      format_(format) {}

V4L2H264EncoderImpl::~V4L2H264EncoderImpl() {
  Release();
}

// ---------------------------------------------------------------------------
// Histogram helpers (one-shot)
// ---------------------------------------------------------------------------

void V4L2H264EncoderImpl::ReportInit() {
  if (has_reported_init_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.V4L2H264EncoderImpl.Event",
                            kV4L2H264EncoderEventInit,
                            kV4L2H264EncoderEventMax);
  has_reported_init_ = true;
}

void V4L2H264EncoderImpl::ReportError() {
  if (has_reported_error_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.V4L2H264EncoderImpl.Event",
                            kV4L2H264EncoderEventError,
                            kV4L2H264EncoderEventMax);
  has_reported_error_ = true;
}

// ---------------------------------------------------------------------------
// Initialization / release
// ---------------------------------------------------------------------------

int32_t V4L2H264EncoderImpl::InitEncode(const VideoCodec* inst,
                                          const VideoEncoder::Settings& settings) {
  // --- Validate parameters ---

  if (!inst || inst->codecType != kVideoCodecH264) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0 || inst->width < 1 || inst->height < 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;

  // Ensure simulcastStream[0] is populated even without simulcast so
  // that downstream code can always reference layer 0 safely.
  if (codec_.numberOfSimulcastStreams == 0) {
    codec_.simulcastStream[0].width = codec_.width;
    codec_.simulcastStream[0].height = codec_.height;
  }

  // --- Pre-allocate the encoded image buffer ---

  const size_t initial_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(initial_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  // --- Populate layer configuration ---

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval = codec_.H264()->keyFrameInterval;
  configuration_.width = codec_.width;
  configuration_.height = codec_.height;
  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  // --- Initialize the V4L2 hardware encoder ---

  if (!encoder_->IsInitialized()) {
    // Use the keyframe interval from codec settings if available;
    // otherwise default to ~2 seconds so that late-joining subscribers
    // (or those recovering from packet loss) resync quickly.
    int kf_interval = codec_.H264()->keyFrameInterval;
    if (kf_interval <= 0) {
      kf_interval = codec_.maxFramerate > 0
                        ? codec_.maxFramerate * 2  // ~2 seconds worth of frames
                        : 60;
    }

    if (!encoder_->Initialize(codec_.width, codec_.height,
                               codec_.startBitrate * 1000, kf_interval,
                               codec_.maxFramerate)) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to initialize H.264 encoder";
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  // Kick off rate control with the initial bitrate allocation.
  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t V4L2H264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t V4L2H264EncoderImpl::Release() {
  if (encoder_->IsInitialized())
    encoder_->Destroy();
  return WEBRTC_VIDEO_CODEC_OK;
}

// ---------------------------------------------------------------------------
// Frame encoding
// ---------------------------------------------------------------------------

int32_t V4L2H264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_ || !encoder_->IsInitialized()) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING) << "V4L2: Encode() called before "
                           "RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  // Convert the incoming frame to I420 (may already be I420).
  scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to convert "
                      << VideoFrameBufferTypeToString(
                             input_frame.video_frame_buffer()->type())
                      << " to I420";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  RTC_CHECK(frame_buffer->type() == VideoFrameBuffer::Type::kI420 ||
            frame_buffer->type() == VideoFrameBuffer::Type::kI420A);
  RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
  RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

  if (!configuration_.sending)
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;

  // Skip empty frames.
  if (frame_types && (*frame_types)[0] == VideoFrameType::kEmptyFrame)
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;

  // Determine whether we need to force an IDR keyframe.
  bool send_key_frame =
      (configuration_.key_frame_request && configuration_.sending) ||
      (frame_types && (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (send_key_frame)
    configuration_.key_frame_request = false;

  // --- Encode via V4L2 ---

  std::vector<uint8_t> bitstream;
  bool ok = encoder_->Encode(
      frame_buffer->DataY(), frame_buffer->DataU(), frame_buffer->DataV(),
      frame_buffer->StrideY(), frame_buffer->StrideU(),
      frame_buffer->StrideV(), send_key_frame, bitstream);

  if (!ok || bitstream.empty()) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode() failed";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // --- Populate the EncodedImage and deliver it ---

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(bitstream.data(), bitstream.size()));

  // Parse the bitstream to extract QP for rate-control feedback.
  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ =
      h264_bitstream_parser_.GetLastSliceQp().value_or(-1);

  encoded_image_._encodedWidth = configuration_.width;
  encoded_image_._encodedHeight = configuration_.height;
  encoded_image_.SetRtpTimestamp(input_frame.rtp_timestamp());
  encoded_image_.SetColorSpace(input_frame.color_space());
  encoded_image_._frameType = send_key_frame
                                  ? VideoFrameType::kVideoFrameKey
                                  : VideoFrameType::kVideoFrameDelta;

  CodecSpecificInfo codec_specific;
  codec_specific.codecType = kVideoCodecH264;
  codec_specific.codecSpecific.H264.packetization_mode = packetization_mode_;
  codec_specific.codecSpecific.H264.temporal_idx = kNoTemporalIdx;
  codec_specific.codecSpecific.H264.base_layer_sync = false;
  codec_specific.codecSpecific.H264.idr_frame = send_key_frame;

  encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_specific);

  return WEBRTC_VIDEO_CODEC_OK;
}

// ---------------------------------------------------------------------------
// Encoder info / rate control
// ---------------------------------------------------------------------------

VideoEncoder::EncoderInfo V4L2H264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "V4L2 H264 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void V4L2H264EncoderImpl::SetRates(
    const RateControlParameters& parameters) {
  if (!encoder_) {
    RTC_LOG(LS_WARNING) << "V4L2: SetRates() called on null encoder";
    return;
  }

  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "V4L2: Invalid framerate: "
                        << parameters.framerate_fps;
    return;
  }

  // Zero bitrate means "pause the stream".
  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);
  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
    encoder_->UpdateRates(configuration_.max_frame_rate,
                          configuration_.target_bps);
  } else {
    configuration_.SetStreamState(false);
  }
}

void V4L2H264EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  // Request a keyframe when resuming so the receiver can resync.
  if (send_stream && !sending)
    key_frame_request = true;
  sending = send_stream;
}

}  // namespace webrtc
