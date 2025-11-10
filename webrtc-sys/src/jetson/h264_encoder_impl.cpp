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
#include <memory>
#include <vector>
#include <optional>

#include "absl/strings/match.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/svc/create_scalability_structure.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "modules/video_coding/utility/simulcast_utility.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"
#include "system_wrappers/include/metrics.h"
#include "third_party/libyuv/include/libyuv/convert.h"
#include "third_party/libyuv/include/libyuv/scale.h"

#if defined(USE_JETSON_VIDEO_CODEC)
// Nvidia Jetson Multimedia API (L4T MMAPI)
// Reference: Jetson Linux Multimedia API
// https://docs.nvidia.com/jetson/l4t-multimedia/mmapi_group.html
#include <errno.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "NvVideoEncoder.h"

namespace livekit {
// Minimal session wrapper around NvVideoEncoder using USERPTR for CPU I420.
class JetsonV4L2Session {
 public:
  JetsonV4L2Session() = default;
  ~JetsonV4L2Session() { Destroy(); }

  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int framerate,
                  int idr_interval,
                  webrtc::H264Profile profile) {
    width_ = width;
    height_ = height;

    // Create encoder instance (device node auto-selected by MMAPI).
    enc_.reset(NvVideoEncoder::createVideoEncoder("webrtc-jetson-enc"));
    if (!enc_) return false;

    // Output plane: raw input frames (I420 planar) with USERPTR.
    if (enc_->setCapturePlaneFormat(V4L2_PIX_FMT_H264, width, height, 2 * 1024 * 1024) < 0) {
      return false;
    }
    if (enc_->setOutputPlaneFormat(V4L2_PIX_FMT_YUV420M, width, height) < 0) {
      return false;
    }

    // Rate control and profile.
    // Bitrate in bps.
    if (enc_->setBitrate(bitrate) < 0) {
      return false;
    }
    // IDR interval (GOP).
    if (enc_->setIFrameInterval(idr_interval) < 0) {
      return false;
    }

    // Set frame rate (num/den).
    if (enc_->setFrameRate(framerate, 1) < 0) {
      return false;
    }

    // H264 profile.
    v4l2_control ctrl {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_H264_PROFILE;
    switch (profile) {
      case webrtc::H264Profile::kProfileConstrainedBaseline:
      case webrtc::H264Profile::kProfileBaseline:
        ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE;
        break;
      case webrtc::H264Profile::kProfileMain:
        ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_MAIN;
        break;
      case webrtc::H264Profile::kProfileConstrainedHigh:
      case webrtc::H264Profile::kProfileHigh:
        ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_HIGH;
        break;
    }
    if (enc_->setExtControls(&ctrl, 1) < 0) {
      // Some Jetson builds may not support changing profile; continue best-effort.
    }

    // CBR rate control if available.
    v4l2_control rc_ctrl {};
    rc_ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE_MODE;
    rc_ctrl.value = V4L2_MPEG_VIDEO_BITRATE_MODE_CBR;
    enc_->setExtControls(&rc_ctrl, 1);

    // Initialize planes/buffers. Use a small queue size; we run synchronously.
    if (enc_->output_plane.setupPlane(V4L2_MEMORY_USERPTR, kNumOutputBuffers, true, false) < 0) {
      return false;
    }
    if (enc_->capture_plane.setupPlane(V4L2_MEMORY_MMAP, kNumCaptureBuffers, true, false) < 0) {
      return false;
    }
    if (enc_->subscribeEvent(V4L2_EVENT_EOS, 0) < 0) {
      // Not fatal.
    }
    if (enc_->output_plane.setStreamStatus(true) < 0) {
      return false;
    }
    if (enc_->capture_plane.setStreamStatus(true) < 0) {
      return false;
    }
    // Pre-queue capture plane buffers to receive bitstream.
    for (uint32_t i = 0; i < enc_->capture_plane.getNumBuffers(); ++i) {
      struct v4l2_buffer v4l2_buf {};
      struct v4l2_plane planes[VIDEO_MAX_PLANES] {};
      v4l2_buf.index = i;
      v4l2_buf.m.planes = planes;
      v4l2_buf.length = enc_->capture_plane.getNumPlanes();
      if (enc_->capture_plane.qBuffer(v4l2_buf, nullptr) < 0) {
        return false;
      }
    }
    return true;
  }

  void Destroy() {
    if (enc_) {
      enc_->capture_plane.setStreamStatus(false);
      enc_->output_plane.setStreamStatus(false);
      enc_.reset();
    }
  }

  bool EncodeI420(const uint8_t* y,
                  const uint8_t* u,
                  const uint8_t* v,
                  bool force_idr,
                  std::vector<uint8_t>& out) {
    if (!enc_) return false;

    // Output plane buffer for raw input.
    struct v4l2_buffer v4l2_buf {};
    struct v4l2_plane planes[VIDEO_MAX_PLANES] {};
    v4l2_buf.m.planes = planes;
    v4l2_buf.length = enc_->output_plane.getNumPlanes();

    if (enc_->output_plane.getNumBuffers() == 0) {
      return false;
    }

    if (enc_->output_plane.getEmpty(v4l2_buf, nullptr) < 0) {
      return false;
    }

    // Fill planes with user pointers to the provided I420 data.
    const int y_stride = width_;
    const int uv_stride = width_ / 2;
    const int y_size = y_stride * height_;
    const int u_size = uv_stride * (height_ / 2);
    const int v_size = u_size;

    // Y plane
    planes[0].bytesused = y_size;
    planes[0].m.userptr = reinterpret_cast<unsigned long>(const_cast<uint8_t*>(y));
    planes[0].length = y_size;
    // U plane
    planes[1].bytesused = u_size;
    planes[1].m.userptr = reinterpret_cast<unsigned long>(const_cast<uint8_t*>(u));
    planes[1].length = u_size;
    // V plane
    planes[2].bytesused = v_size;
    planes[2].m.userptr = reinterpret_cast<unsigned long>(const_cast<uint8_t*>(v));
    planes[2].length = v_size;

    // Force IDR via control if requested.
    if (force_idr) {
      v4l2_control ctrl {};
      ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_IDR;
      ctrl.value = 1;
      enc_->setExtControls(&ctrl, 1);
    }

    // Queue raw frame.
    if (enc_->output_plane.qBuffer(v4l2_buf, nullptr) < 0) {
      return false;
    }

    // Dequeue capture plane for encoded bitstream (blocking).
    struct v4l2_buffer cap_buf {};
    struct v4l2_plane cap_planes[VIDEO_MAX_PLANES] {};
    cap_buf.m.planes = cap_planes;
    cap_buf.length = enc_->capture_plane.getNumPlanes();

    if (enc_->capture_plane.dqBuffer(cap_buf, nullptr, -1) < 0) {
      return false;
    }

    // Copy out encoded payload from capture plane.
    const uint8_t* cap_data =
        static_cast<uint8_t*>(enc_->capture_plane.getNthBuffer(cap_buf.index)->planes[0].data);
    size_t cap_size = cap_buf.m.planes[0].bytesused;
    out.assign(cap_data, cap_data + cap_size);

    // Re-queue capture buffer.
    if (enc_->capture_plane.qBuffer(cap_buf, nullptr) < 0) {
      return false;
    }
    return true;
  }

 private:
  static constexpr uint32_t kNumOutputBuffers = 4;
  static constexpr uint32_t kNumCaptureBuffers = 4;
  int width_ = 0;
  int height_ = 0;
  std::unique_ptr<NvVideoEncoder> enc_;
};
}  // namespace livekit
#endif  // defined(USE_JETSON_VIDEO_CODEC)

namespace webrtc {

enum JetsonH264EncoderImplEvent {
  kH264EncoderEventInit = 0,
  kH264EncoderEventError = 1,
  kH264EncoderEventMax = 16,
};

JetsonH264EncoderImpl::JetsonH264EncoderImpl(const webrtc::Environment& env,
                                             const SdpVideoFormat& format)
    : env_(env),
      packetization_mode_(
          H264EncoderSettings::Parse(format).packetization_mode),
      format_(format) {
  std::string hexString = format_.parameters.at("profile-level-id");
  std::optional<webrtc::H264ProfileLevelId> profile_level_id =
      webrtc::ParseH264ProfileLevelId(hexString.c_str());
  if (profile_level_id.has_value()) {
    profile_ = profile_level_id->profile;
    level_ = profile_level_id->level;
  }
}

JetsonH264EncoderImpl::~JetsonH264EncoderImpl() {
  Release();
}

void JetsonH264EncoderImpl::ReportInit() {
  if (has_reported_init_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.JetsonH264EncoderImpl.Event",
                            kH264EncoderEventInit, kH264EncoderEventMax);
  has_reported_init_ = true;
}

void JetsonH264EncoderImpl::ReportError() {
  if (has_reported_error_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.JetsonH264EncoderImpl.Event",
                            kH264EncoderEventError, kH264EncoderEventMax);
  has_reported_error_ = true;
}

int32_t JetsonH264EncoderImpl::InitEncode(const VideoCodec* inst,
                                          const VideoEncoder::Settings& settings) {
  if (!inst || inst->codecType != kVideoCodecH264) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;

  if (codec_.numberOfSimulcastStreams == 0) {
    codec_.simulcastStream[0].width = codec_.width;
    codec_.simulcastStream[0].height = codec_.height;
  }

  const size_t new_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(new_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval = codec_.H264()->keyFrameInterval;

  configuration_.width = codec_.width;
  configuration_.height = codec_.height;

  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

#if defined(USE_JETSON_VIDEO_CODEC)
  if (!configuration_.sending) {
    // Initialize hardware encoder session.
    const int keyInterval = codec_.maxFramerate > 0 ? codec_.maxFramerate * 5 : 60;
    jetson_session_ = std::make_unique<livekit::JetsonV4L2Session>();
    if (!jetson_session_->Initialize(codec_.width, codec_.height,
                                     configuration_.target_bps,
                                     codec_.maxFramerate,
                                     keyInterval, profile_)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize Jetson V4L2 session.";
      ReportError();
      jetson_session_.reset();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }
#endif

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  ReportInit();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Release() {
#if defined(USE_JETSON_VIDEO_CODEC)
  jetson_session_.reset();
#endif
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  // For now we only accept CPU buffers (I420).
  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert "
                      << VideoFrameBufferTypeToString(
                             input_frame.video_frame_buffer()->type())
                      << " image to I420. Can't encode frame.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }

  bool send_key_frame =
      is_keyframe_needed ||
      (frame_types && (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (send_key_frame) {
    is_keyframe_needed = true;
    configuration_.key_frame_request = false;
  }

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (frame_types != nullptr) {
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

#if defined(USE_JETSON_VIDEO_CODEC)
  if (!jetson_session_) {
    RTC_LOG(LS_ERROR) << "Jetson session not initialized.";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  std::vector<uint8_t> output;
  bool ok = jetson_session_->EncodeI420(frame_buffer->DataY(), frame_buffer->DataU(),
                                        frame_buffer->DataV(), send_key_frame, output);
  if (!ok || output.empty()) {
    RTC_LOG(LS_ERROR) << "Jetson V4L2 encode failed.";
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(output.data(), output.size()));

  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ = h264_bitstream_parser_.GetLastSliceQp().value_or(-1);
  encoded_image_._encodedWidth = configuration_.width;
  encoded_image_._encodedHeight = configuration_.height;
  encoded_image_.SetRtpTimestamp(input_frame.rtp_timestamp());
  encoded_image_.SetColorSpace(input_frame.color_space());
  encoded_image_._frameType = send_key_frame ? VideoFrameType::kVideoFrameKey
                                             : VideoFrameType::kVideoFrameDelta;
  CodecSpecificInfo codec_specific;
  codec_specific.codecType = kVideoCodecH264;
  codec_specific.codecSpecific.H264.packetization_mode = packetization_mode_;
  codec_specific.codecSpecific.H264.temporal_idx = kNoTemporalIdx;
  codec_specific.codecSpecific.H264.base_layer_sync = false;
  codec_specific.codecSpecific.H264.idr_frame = send_key_frame;
  encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_specific);
  return WEBRTC_VIDEO_CODEC_OK;
#else
  RTC_LOG(LS_ERROR) << "Jetson encoder not enabled at build time.";
  ReportError();
  return WEBRTC_VIDEO_CODEC_ERROR;
#endif
}

VideoEncoder::EncoderInfo JetsonH264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "Jetson V4L2 H264 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void JetsonH264EncoderImpl::SetRates(
    const RateControlParameters& parameters) {
  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);

  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
    // Note: updating runtime rate control requires MMAPI control calls;
    // keep simple for now. Future work: adjust bitrate and framerate here.
  } else {
    configuration_.SetStreamState(false);
  }
}

void JetsonH264EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc


