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
#include <cmath>
#include <cstdint>
#include <limits>
#include <string>
#include <utility>

#include "api/video/video_codec_constants.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "modules/video_coding/utility/simulcast_utility.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "system_wrappers/include/metrics.h"

#include "dmabuf_video_frame_buffer.h"

namespace webrtc {

namespace {

constexpr uint32_t kFourccYuv420 = 0x32315559u;  // V4L2_PIX_FMT_YUV420
constexpr uint32_t kFourccNv12 = 0x3231564Eu;    // V4L2_PIX_FMT_NV12
constexpr int kH264MacroblockAlignment = 16;

int AlignDown(int value, int alignment) {
  return value / alignment * alignment;
}

std::pair<int, int> MacroblockAlignedEncodeSize(int width, int height) {
  if (width < kH264MacroblockAlignment || height < kH264MacroblockAlignment ||
      (width % kH264MacroblockAlignment == 0 &&
       height % kH264MacroblockAlignment == 0)) {
    return {width, height};
  }

  const double source_aspect = static_cast<double>(width) / height;
  const int64_t source_area = static_cast<int64_t>(width) * height;
  const std::pair<int, int> largest_aligned = {
      AlignDown(width, kH264MacroblockAlignment),
      AlignDown(height, kH264MacroblockAlignment)};

  std::pair<int, int> best_aspect_match = {0, 0};
  int64_t best_aspect_area = 0;
  for (int candidate_height = kH264MacroblockAlignment;
       candidate_height <= height;
       candidate_height += kH264MacroblockAlignment) {
    const int candidate_width =
        AlignDown(static_cast<int>(
                      std::round(candidate_height * source_aspect)),
                  kH264MacroblockAlignment);
    if (candidate_width <= 0 || candidate_width > width)
      continue;

    const double candidate_aspect =
        static_cast<double>(candidate_width) / candidate_height;
    const double aspect_error = std::abs(candidate_aspect - source_aspect);
    const int64_t candidate_area =
        static_cast<int64_t>(candidate_width) * candidate_height;
    if (aspect_error < 0.001 && candidate_area > best_aspect_area) {
      best_aspect_match = {candidate_width, candidate_height};
      best_aspect_area = candidate_area;
    }
  }

  // Prefer an aspect-perfect rung when it is still close to the requested
  // size. This maps 480x360 to 448x336, avoiding bcm2835-codec's bad 360px
  // visible height without needlessly shrinking large 16:9 sizes such as
  // 1920x1080 down to 1536x864.
  if (best_aspect_area * 5 >= source_area * 4) {
    return best_aspect_match;
  }

  return largest_aligned;
}

int PostReconfigureDropFrameCount(float frame_rate) {
  const int nominal_frame_rate = static_cast<int>(std::round(frame_rate));
  // Drop roughly half a second of encoder output, capped to keep adaptation
  // responsive. bcm2835-codec can emit parseable but visually corrupt frames
  // immediately after a size reconfiguration; the released frame after this
  // window is forced to be a fresh IDR.
  return std::min(15, std::max(4, nominal_frame_rate / 2));
}

int ChromaHeight(int height) {
  return (height + 1) / 2;
}

bool IsContiguousDmabufLayout(
    const livekit_ffi::DmabufVideoFrameBuffer& buffer) {
  const int width = buffer.width();
  const int height = buffer.height();
  if (buffer.num_planes() == 0)
    return false;
  const int stride_y = buffer.plane_stride(0);
  if (stride_y < width || buffer.plane_offset(0) > buffer.total_size())
    return false;

  const size_t base = buffer.plane_offset(0);
  const int chroma_height = ChromaHeight(height);
  if (buffer.fourcc() == kFourccYuv420) {
    if (buffer.num_planes() < 3 || (stride_y % 2) != 0)
      return false;
    const int stride_uv = stride_y / 2;
    const size_t u_offset = base + static_cast<size_t>(stride_y) * height;
    const size_t v_offset =
        u_offset + static_cast<size_t>(stride_uv) * chroma_height;
    const size_t end = v_offset + static_cast<size_t>(stride_uv) * chroma_height;
    return buffer.plane_stride(1) == stride_uv &&
           buffer.plane_stride(2) == stride_uv &&
           buffer.plane_offset(1) == u_offset &&
           buffer.plane_offset(2) == v_offset &&
           end <= buffer.total_size();
  }

  if (buffer.fourcc() == kFourccNv12) {
    if (buffer.num_planes() < 2)
      return false;
    const size_t uv_offset = base + static_cast<size_t>(stride_y) * height;
    const size_t end =
        uv_offset + static_cast<size_t>(stride_y) * chroma_height;
    return buffer.plane_stride(1) == stride_y &&
           buffer.plane_offset(1) == uv_offset &&
           end <= buffer.total_size();
  }

  return false;
}

}  // namespace

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
  first_frame_ = true;
  post_reconfigure_drop_frames_ = 0;

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

  // The encoder's output-queue memory mode is decided lazily when the
  // first frame arrives in Encode(), so we don't initialize the hardware
  // here. This lets us pick DMABUF when the source actually provides
  // dmabuf-backed native buffers, falling back to MMAP otherwise.

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
  first_frame_ = true;
  post_reconfigure_drop_frames_ = 0;
  return WEBRTC_VIDEO_CODEC_OK;
}

// ---------------------------------------------------------------------------
// Frame encoding
// ---------------------------------------------------------------------------

int32_t V4L2H264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING) << "V4L2: Encode() called before "
                           "RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  if (!configuration_.sending)
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;

  // Skip empty frames.
  if (frame_types && (*frame_types)[0] == VideoFrameType::kEmptyFrame)
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;

  const int frame_width = input_frame.width();
  const int frame_height = input_frame.height();
  if (frame_width <= 0 || frame_height <= 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Invalid input frame size " << frame_width
                      << "x" << frame_height;
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  const std::pair<int, int> encode_size =
      MacroblockAlignedEncodeSize(frame_width, frame_height);
  const int encode_width = encode_size.first;
  const int encode_height = encode_size.second;
  const bool frame_size_adjusted = encode_width != frame_width ||
                                   encode_height != frame_height;
  const bool frame_size_changed = encode_width != configuration_.width ||
                                  encode_height != configuration_.height;
  if (frame_size_changed) {
    RTC_LOG(LS_INFO) << "V4L2: Reconfiguring H.264 encoder from "
                     << configuration_.width << "x" << configuration_.height
                     << " to " << encode_width << "x" << encode_height;
    if (frame_size_adjusted) {
      RTC_LOG(LS_INFO)
          << "V4L2: Scaling input frame " << frame_width << "x"
          << frame_height << " to macroblock-aligned " << encode_width << "x"
          << encode_height;
    }
    if (encoder_->IsInitialized())
      encoder_->Destroy();

    configuration_.width = encode_width;
    configuration_.height = encode_height;
    codec_.width = encode_width;
    codec_.height = encode_height;
    codec_.simulcastStream[0].width = encode_width;
    codec_.simulcastStream[0].height = encode_height;

    encoded_image_.SetEncodedData(EncodedImageBuffer::Create(
        CalcBufferSize(VideoType::kI420, encode_width, encode_height)));
    encoded_image_._encodedWidth = encode_width;
    encoded_image_._encodedHeight = encode_height;
    encoded_image_.set_size(0);

    post_reconfigure_drop_frames_ =
        PostReconfigureDropFrameCount(configuration_.max_frame_rate);
    RTC_LOG(LS_INFO) << "V4L2: Dropping "
                     << post_reconfigure_drop_frames_
                     << " post-reconfigure warmup frames";
  }

  // Determine whether we need to force an IDR keyframe.
  bool send_key_frame =
      frame_size_changed ||
      (configuration_.key_frame_request && configuration_.sending) ||
      (frame_types && (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (first_frame_) {
    send_key_frame = true;
    first_frame_ = false;
  }
  if (send_key_frame)
    configuration_.key_frame_request = false;

  // Probe the incoming buffer: if it's a DMABUF native buffer, we can
  // import it directly into the encoder's OUTPUT queue. Otherwise we
  // fall back to ToI420 + MMAP.
  auto* native_dmabuf = livekit_ffi::DmabufVideoFrameBuffer::TryCast(
      input_frame.video_frame_buffer().get());
  bool native_dmabuf_safe =
      !frame_size_adjusted && native_dmabuf &&
      IsContiguousDmabufLayout(*native_dmabuf);
  if (native_dmabuf_safe && encoder_->IsInitialized() &&
      encoder_->mode() == livekit_ffi::OutputBufferMode::Dmabuf &&
      (native_dmabuf->fourcc() != encoder_->output_fourcc() ||
       native_dmabuf->plane_stride(0) != encoder_->output_stride())) {
    native_dmabuf_safe = false;
  }
  if (native_dmabuf && !native_dmabuf_safe && frame_size_changed) {
    RTC_LOG(LS_WARNING)
        << "V4L2: Native DMABUF layout cannot be used directly"
        << (frame_size_adjusted ? " after macroblock alignment" : "")
        << "; falling back to ToI420 + MMAP";
  }

  auto initialize_encoder = [&](livekit_ffi::OutputBufferMode desired,
                                uint32_t fourcc,
                                int input_stride) -> bool {
    int kf_interval = codec_.H264()->keyFrameInterval;
    if (kf_interval <= 0) {
      kf_interval = codec_.maxFramerate > 0
                        ? codec_.maxFramerate * 2  // ~2 seconds of frames
                        : 60;
    }
    return encoder_->Initialize(configuration_.width, configuration_.height,
                                codec_.startBitrate * 1000, kf_interval,
                                codec_.maxFramerate, desired, fourcc,
                                input_stride);
  };

  // Lazily initialize the hardware encoder, picking the mode based on
  // the first frame seen. The mode is then locked for the lifetime of
  // the encoder; mode changes (which would require destroy + reinit)
  // are logged and silently routed through ToI420.
  if (!encoder_->IsInitialized()) {
    // CPU-backed frames are fed through MMAP for driver compatibility. USERPTR
    // avoids one copy when planes are already contiguous, but bcm2835-codec on
    // Raspberry Pi is much happier with driver-owned MMAP buffers. Native
    // DMABUF frames use zero-copy only when the single-planar V4L2 encoder can
    // represent their layout exactly.
    livekit_ffi::OutputBufferMode desired =
        native_dmabuf_safe ? livekit_ffi::OutputBufferMode::Dmabuf
                           : livekit_ffi::OutputBufferMode::Mmap;
    uint32_t fourcc = native_dmabuf_safe ? native_dmabuf->fourcc()
                                         : kFourccYuv420;
    int input_stride = native_dmabuf_safe ? native_dmabuf->plane_stride(0)
                                          : configuration_.width;
    if (!initialize_encoder(desired, fourcc, input_stride)) {
      if (desired != livekit_ffi::OutputBufferMode::Dmabuf) {
        RTC_LOG(LS_ERROR) << "V4L2: Failed to initialize H.264 encoder";
        first_frame_ = true;
        ReportError();
        return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      }

      RTC_LOG(LS_WARNING)
          << "V4L2: Failed to initialize DMABUF import; falling back to "
             "ToI420 + MMAP";
      native_dmabuf_safe = false;
      send_key_frame = true;
      desired = livekit_ffi::OutputBufferMode::Mmap;
      fourcc = kFourccYuv420;
      input_stride = configuration_.width;
      if (!initialize_encoder(desired, fourcc, input_stride)) {
        RTC_LOG(LS_ERROR) << "V4L2: Failed to initialize MMAP fallback";
        first_frame_ = true;
        ReportError();
        return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      }
    }
    if (desired == livekit_ffi::OutputBufferMode::Dmabuf &&
        (encoder_->output_fourcc() != fourcc ||
         encoder_->output_stride() != input_stride)) {
      RTC_LOG(LS_WARNING)
          << "V4L2: Driver adjusted DMABUF output format/stride; requested "
          << fourcc << "/" << input_stride << ", negotiated "
          << encoder_->output_fourcc() << "/" << encoder_->output_stride()
          << "; falling back to ToI420 + MMAP";
      encoder_->Destroy();
      native_dmabuf_safe = false;
      send_key_frame = true;
      if (!initialize_encoder(livekit_ffi::OutputBufferMode::Mmap,
                              kFourccYuv420, configuration_.width)) {
        RTC_LOG(LS_ERROR) << "V4L2: Failed to initialize MMAP fallback";
        first_frame_ = true;
        ReportError();
        return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
      }
    }
  } else if (encoder_->mode() == livekit_ffi::OutputBufferMode::Dmabuf &&
             !native_dmabuf_safe) {
    RTC_LOG(LS_WARNING)
        << "V4L2: Switching from DMABUF to MMAP because the incoming frame "
           "cannot be safely imported";
    encoder_->Destroy();
    send_key_frame = true;
    if (!initialize_encoder(livekit_ffi::OutputBufferMode::Mmap,
                            kFourccYuv420, configuration_.width)) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to reinitialize H.264 encoder";
      first_frame_ = true;
      ReportError();
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    }
  }

  // --- Encode via V4L2 ---

  livekit_ffi::EncodeResult result;
  if (native_dmabuf_safe &&
      encoder_->mode() == livekit_ffi::OutputBufferMode::Dmabuf) {
    result = encoder_->EncodeDmabuf(native_dmabuf->dmabuf_fd(),
                                    native_dmabuf->plane_offset(0),
                                    native_dmabuf->total_size(),
                                    send_key_frame,
                                    input_frame.rtp_timestamp());
  } else {
    if (native_dmabuf && frame_size_changed) {
      RTC_LOG(LS_WARNING) << "V4L2: Native DMABUF frame received but encoder "
                             "is in non-DMABUF mode; falling back to ToI420";
    }
    scoped_refptr<VideoFrameBuffer> encode_buffer =
        input_frame.video_frame_buffer();
    if (frame_size_adjusted) {
      encode_buffer = encode_buffer->CropAndScale(
          /*offset_x=*/0, /*offset_y=*/0, frame_width, frame_height,
          encode_width, encode_height);
      if (!encode_buffer) {
        RTC_LOG(LS_ERROR) << "V4L2: Failed to scale frame " << frame_width
                          << "x" << frame_height << " to " << encode_width
                          << "x" << encode_height;
        return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
      }
    }
    scoped_refptr<I420BufferInterface> frame_buffer = encode_buffer->ToI420();
    if (!frame_buffer) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to convert "
                        << VideoFrameBufferTypeToString(
                               input_frame.video_frame_buffer()->type())
                        << " to I420";
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    if (configuration_.width != frame_buffer->width() ||
        configuration_.height != frame_buffer->height()) {
      RTC_LOG(LS_ERROR) << "V4L2: Converted frame size "
                        << frame_buffer->width() << "x"
                        << frame_buffer->height()
                        << " does not match configured encoder size "
                        << configuration_.width << "x"
                        << configuration_.height;
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
    result = encoder_->Encode(
        frame_buffer->DataY(), frame_buffer->DataU(), frame_buffer->DataV(),
        frame_buffer->StrideY(), frame_buffer->StrideU(),
        frame_buffer->StrideV(), send_key_frame, input_frame.rtp_timestamp());
  }

  if (result.status == livekit_ffi::EncodeResult::Status::NoOutput) {
    return WEBRTC_VIDEO_CODEC_OK;
  }
  if (result.status == livekit_ffi::EncodeResult::Status::Error ||
      !result.frame.bitstream || result.frame.bitstream->size() == 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode() failed";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  if (post_reconfigure_drop_frames_ > 0) {
    --post_reconfigure_drop_frames_;
    configuration_.key_frame_request = true;
    RTC_LOG(LS_INFO) << "V4L2: Dropping post-reconfigure warmup frame; "
                     << post_reconfigure_drop_frames_ << " remaining";
    return WEBRTC_VIDEO_CODEC_OK;
  }

  // --- Populate the EncodedImage and deliver it ---

  encoded_image_.SetEncodedData(result.frame.bitstream);

  // Parse the bitstream to extract QP for rate-control feedback.
  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ =
      h264_bitstream_parser_.GetLastSliceQp().value_or(-1);

  encoded_image_._encodedWidth = configuration_.width;
  encoded_image_._encodedHeight = configuration_.height;
  encoded_image_.SetRtpTimestamp(result.frame.rtp_timestamp);
  encoded_image_.SetColorSpace(input_frame.color_space());
  encoded_image_._frameType = result.frame.key_frame
                                  ? VideoFrameType::kVideoFrameKey
                                  : VideoFrameType::kVideoFrameDelta;

  CodecSpecificInfo codec_specific;
  codec_specific.codecType = kVideoCodecH264;
  codec_specific.codecSpecific.H264.packetization_mode = packetization_mode_;
  codec_specific.codecSpecific.H264.temporal_idx = kNoTemporalIdx;
  codec_specific.codecSpecific.H264.base_layer_sync = false;
  codec_specific.codecSpecific.H264.idr_frame = result.frame.key_frame;

  encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_specific);

  return WEBRTC_VIDEO_CODEC_OK;
}

// ---------------------------------------------------------------------------
// Encoder info / rate control
// ---------------------------------------------------------------------------

VideoEncoder::EncoderInfo V4L2H264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  // Accept DMABUF-backed native frames so the V4L2 encoder can import
  // them directly via V4L2_MEMORY_DMABUF (see livekit_ffi::DmabufVideoFrameBuffer).
  info.supports_native_handle = true;
  info.implementation_name = "V4L2 H264 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  // bcm2835-codec's H.264 path is unreliable for visible frame sizes that are
  // not macroblock aligned. Tell WebRTC's source adapter to avoid rungs such as
  // 480x360 and prefer dimensions that are cleanly divisible by 16.
  info.requested_resolution_alignment = 16;
  info.apply_alignment_to_all_simulcast_layers = true;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kNative,
                                  VideoFrameBuffer::Type::kI420};
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
