#include "v4l2_h265_encoder_impl.h"

#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#include <linux/videodev2.h>
#include <string.h>

#include <algorithm>
#include <limits>
#include <string>

#include "absl/strings/match.h"
#include "absl/types/optional.h"
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

namespace webrtc {

// Used by histograms. Values of entries should not be changed.
enum H265EncoderImplEvent {
  kH265EncoderEventInit = 0,
  kH265EncoderEventError = 1,
  kH265EncoderEventMax = 16,
};

V4L2H265EncoderImpl::V4L2H265EncoderImpl(
    const webrtc::Environment& env,
    const std::string& device_path,
    const SdpVideoFormat& format)
    : env_(env),
      device_path_(device_path),
      format_(format) {
}

V4L2H265EncoderImpl::~V4L2H265EncoderImpl() {
  Release();
}

void V4L2H265EncoderImpl::ReportInit() {
  if (has_reported_init_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventInit, kH265EncoderEventMax);
  has_reported_init_ = true;
}

void V4L2H265EncoderImpl::ReportError() {
  if (has_reported_error_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventError, kH265EncoderEventMax);
  has_reported_error_ = true;
}

bool V4L2H265EncoderImpl::InitializeV4L2Device() {
  RTC_LOG(LS_INFO) << "V4L2 H265 Encoder: Opening device " << device_path_;
  
  device_fd_ = open(device_path_.c_str(), O_RDWR | O_NONBLOCK);
  if (device_fd_ < 0) {
    RTC_LOG(LS_ERROR) << "Failed to open V4L2 device: " << device_path_ 
                      << " error: " << strerror(errno);
    return false;
  }

  // Query capabilities
  struct v4l2_capability cap;
  if (ioctl(device_fd_, VIDIOC_QUERYCAP, &cap) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to query V4L2 capabilities: " << strerror(errno);
    
    // For Jetson devices, sometimes QUERYCAP fails on the symlink or special device node
    // but the device is still usable.
    if (device_path_.find("nvenc") != std::string::npos) {
      RTC_LOG(LS_WARNING) << "Ignoring QUERYCAP failure for Jetson NVENC device";
      return true;
    }

    close(device_fd_);
    device_fd_ = -1;
    return false;
  }

  if (!(cap.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE)) {
    RTC_LOG(LS_ERROR) << "Device does not support M2M MPLANE";

    if (device_path_.find("nvenc") != std::string::npos) {
      RTC_LOG(LS_WARNING) << "Ignoring missing M2M MPLANE capability for Jetson NVENC device";
      return true;
    }

    close(device_fd_);
    device_fd_ = -1;
    return false;
  }

  RTC_LOG(LS_INFO) << "V4L2 device opened successfully: " << cap.card;
  return true;
}

void V4L2H265EncoderImpl::CleanupV4L2Device() {
  if (device_fd_ >= 0) {
    DeallocateBuffers();
    close(device_fd_);
    device_fd_ = -1;
  }
}

bool V4L2H265EncoderImpl::AllocateInputBuffers() {
  struct v4l2_requestbuffers req;
  memset(&req, 0, sizeof(req));
  req.count = 6;
  req.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  req.memory = V4L2_MEMORY_MMAP;

  if (ioctl(device_fd_, VIDIOC_REQBUFS, &req) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to request input buffers";
    return false;
  }

  num_input_buffers_ = req.count;
  input_buffers_.resize(num_input_buffers_);
  input_buffer_sizes_.resize(num_input_buffers_);

  for (uint32_t i = 0; i < num_input_buffers_; i++) {
    struct v4l2_buffer buf;
    struct v4l2_plane planes[3];
    memset(&buf, 0, sizeof(buf));
    memset(planes, 0, sizeof(planes));
    
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.m.planes = planes;
    buf.length = 3;

    if (ioctl(device_fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to query input buffer " << i;
      return false;
    }

    input_buffer_sizes_[i] = buf.m.planes[0].length;
    input_buffers_[i] = mmap(NULL, buf.m.planes[0].length,
                              PROT_READ | PROT_WRITE, MAP_SHARED,
                              device_fd_, buf.m.planes[0].m.mem_offset);
    
    if (input_buffers_[i] == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "Failed to mmap input buffer " << i;
      return false;
    }
  }

  RTC_LOG(LS_INFO) << "Allocated " << num_input_buffers_ << " input buffers";
  return true;
}

bool V4L2H265EncoderImpl::AllocateOutputBuffers() {
  struct v4l2_requestbuffers req;
  memset(&req, 0, sizeof(req));
  req.count = 6;
  req.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  req.memory = V4L2_MEMORY_MMAP;

  if (ioctl(device_fd_, VIDIOC_REQBUFS, &req) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to request output buffers";
    return false;
  }

  num_output_buffers_ = req.count;
  output_buffers_.resize(num_output_buffers_);
  output_buffer_sizes_.resize(num_output_buffers_);

  for (uint32_t i = 0; i < num_output_buffers_; i++) {
    struct v4l2_buffer buf;
    struct v4l2_plane planes[1];
    memset(&buf, 0, sizeof(buf));
    memset(planes, 0, sizeof(planes));
    
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.m.planes = planes;
    buf.length = 1;

    if (ioctl(device_fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to query output buffer " << i;
      return false;
    }

    output_buffer_sizes_[i] = buf.m.planes[0].length;
    output_buffers_[i] = mmap(NULL, buf.m.planes[0].length,
                               PROT_READ | PROT_WRITE, MAP_SHARED,
                               device_fd_, buf.m.planes[0].m.mem_offset);
    
    if (output_buffers_[i] == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "Failed to mmap output buffer " << i;
      return false;
    }

    // Queue the output buffer immediately
    if (ioctl(device_fd_, VIDIOC_QBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to queue output buffer " << i;
      return false;
    }
  }

  RTC_LOG(LS_INFO) << "Allocated " << num_output_buffers_ << " output buffers";
  return true;
}

void V4L2H265EncoderImpl::DeallocateBuffers() {
  for (size_t i = 0; i < input_buffers_.size(); i++) {
    if (input_buffers_[i] != nullptr && input_buffers_[i] != MAP_FAILED) {
      munmap(input_buffers_[i], input_buffer_sizes_[i]);
    }
  }
  input_buffers_.clear();
  input_buffer_sizes_.clear();

  for (size_t i = 0; i < output_buffers_.size(); i++) {
    if (output_buffers_[i] != nullptr && output_buffers_[i] != MAP_FAILED) {
      munmap(output_buffers_[i], output_buffer_sizes_[i]);
    }
  }
  output_buffers_.clear();
  output_buffer_sizes_.clear();
}

int32_t V4L2H265EncoderImpl::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  if (!inst || inst->codecType != kVideoCodecH265) {
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
  configuration_.key_frame_interval = 0;
  configuration_.width = codec_.width;
  configuration_.height = codec_.height;
  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  // Initialize V4L2 device
  if (!InitializeV4L2Device()) {
    RTC_LOG(LS_ERROR) << "Failed to initialize V4L2 device";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Set output format (raw YUV input to encoder)
  memset(&output_format_, 0, sizeof(output_format_));
  output_format_.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  output_format_.fmt.pix_mp.width = codec_.width;
  output_format_.fmt.pix_mp.height = codec_.height;
  output_format_.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_YUV420M;
  output_format_.fmt.pix_mp.field = V4L2_FIELD_ANY;
  output_format_.fmt.pix_mp.num_planes = 3;

  if (ioctl(device_fd_, VIDIOC_S_FMT, &output_format_) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set output format";
    CleanupV4L2Device();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Set capture format (encoded H265 output from encoder)
  memset(&capture_format_, 0, sizeof(capture_format_));
  capture_format_.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  capture_format_.fmt.pix_mp.width = codec_.width;
  capture_format_.fmt.pix_mp.height = codec_.height;
  capture_format_.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_HEVC;
  capture_format_.fmt.pix_mp.field = V4L2_FIELD_ANY;
  capture_format_.fmt.pix_mp.num_planes = 1;

  if (ioctl(device_fd_, VIDIOC_S_FMT, &capture_format_) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set capture format";
    CleanupV4L2Device();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Set bitrate control
  struct v4l2_control ctrl;
  ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE;
  ctrl.value = configuration_.target_bps;
  if (ioctl(device_fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "Failed to set bitrate, continuing anyway";
  }

  // Set frame rate
  struct v4l2_streamparm parm;
  memset(&parm, 0, sizeof(parm));
  parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  parm.parm.output.timeperframe.numerator = 1;
  parm.parm.output.timeperframe.denominator = codec_.maxFramerate;
  if (ioctl(device_fd_, VIDIOC_S_PARM, &parm) < 0) {
    RTC_LOG(LS_WARNING) << "Failed to set framerate, continuing anyway";
  }

  // Set H265 profile
  ctrl.id = V4L2_CID_MPEG_VIDEO_HEVC_PROFILE;
  ctrl.value = V4L2_MPEG_VIDEO_HEVC_PROFILE_MAIN;
  if (ioctl(device_fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "Failed to set H265 profile, continuing anyway";
  }

  // Allocate buffers
  if (!AllocateInputBuffers() || !AllocateOutputBuffers()) {
    RTC_LOG(LS_ERROR) << "Failed to allocate buffers";
    CleanupV4L2Device();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Start streaming
  int type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  if (ioctl(device_fd_, VIDIOC_STREAMON, &type) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start output stream";
    CleanupV4L2Device();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (ioctl(device_fd_, VIDIOC_STREAMON, &type) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start capture stream";
    CleanupV4L2Device();
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  encoder_initialized_ = true;
  frame_count_ = 0;

  RTC_LOG(LS_INFO) << "V4L2 H265/HEVC encoder initialized: "
                   << codec_.width << "x" << codec_.height
                   << " @ " << codec_.maxFramerate << "fps, target_bps="
                   << configuration_.target_bps
                   << " using device " << device_path_;

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  
  ReportInit();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t V4L2H265EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t V4L2H265EncoderImpl::Release() {
  if (!encoder_initialized_) {
    return WEBRTC_VIDEO_CODEC_OK;
  }

  // Stop streaming
  if (device_fd_ >= 0) {
    int type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    ioctl(device_fd_, VIDIOC_STREAMOFF, &type);
    
    type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    ioctl(device_fd_, VIDIOC_STREAMOFF, &type);
  }

  CleanupV4L2Device();
  encoder_initialized_ = false;
  
  return WEBRTC_VIDEO_CODEC_OK;
}

bool V4L2H265EncoderImpl::EncodeFrame(const VideoFrame& frame, bool is_keyframe) {
  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert to I420";
    return false;
  }

  // Dequeue an input buffer
  struct v4l2_buffer buf;
  struct v4l2_plane planes[3];
  memset(&buf, 0, sizeof(buf));
  memset(planes, 0, sizeof(planes));
  
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.m.planes = planes;
  buf.length = 3;

  if (ioctl(device_fd_, VIDIOC_DQBUF, &buf) < 0) {
    if (errno == EAGAIN) {
      return false;
    }
    RTC_LOG(LS_ERROR) << "Failed to dequeue input buffer: " << strerror(errno);
    return false;
  }

  // Copy frame data
  uint8_t* buffer_ptr = static_cast<uint8_t*>(input_buffers_[buf.index]);
  
  int y_size = codec_.width * codec_.height;
  int u_size = (codec_.width / 2) * (codec_.height / 2);
  int v_size = (codec_.width / 2) * (codec_.height / 2);
  
  // Copy Y plane
  uint8_t* y_dst = buffer_ptr;
  const uint8_t* y_src = frame_buffer->DataY();
  for (int i = 0; i < codec_.height; i++) {
    memcpy(y_dst, y_src, codec_.width);
    y_dst += codec_.width;
    y_src += frame_buffer->StrideY();
  }
  
  // Copy U plane
  uint8_t* u_dst = buffer_ptr + y_size;
  const uint8_t* u_src = frame_buffer->DataU();
  for (int i = 0; i < codec_.height / 2; i++) {
    memcpy(u_dst, u_src, codec_.width / 2);
    u_dst += codec_.width / 2;
    u_src += frame_buffer->StrideU();
  }
  
  // Copy V plane
  uint8_t* v_dst = buffer_ptr + y_size + u_size;
  const uint8_t* v_src = frame_buffer->DataV();
  for (int i = 0; i < codec_.height / 2; i++) {
    memcpy(v_dst, v_src, codec_.width / 2);
    v_dst += codec_.width / 2;
    v_src += frame_buffer->StrideV();
  }

  buf.m.planes[0].bytesused = y_size;
  buf.m.planes[1].bytesused = u_size;
  buf.m.planes[2].bytesused = v_size;

  // Request keyframe if needed
  if (is_keyframe) {
    struct v4l2_control ctrl;
    ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
    ctrl.value = 1;
    ioctl(device_fd_, VIDIOC_S_CTRL, &ctrl);
  }

  // Queue the input buffer
  if (ioctl(device_fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to queue input buffer";
    return false;
  }

  return true;
}

int32_t V4L2H265EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_initialized_) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING) << "Encode callback not set";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
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

  current_encoding_is_keyframe_ = is_keyframe_needed;

  // Encode the frame
  if (!EncodeFrame(input_frame, is_keyframe_needed)) {
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Dequeue encoded output
  struct v4l2_buffer buf;
  struct v4l2_plane planes[1];
  memset(&buf, 0, sizeof(buf));
  memset(planes, 0, sizeof(planes));
  
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.m.planes = planes;
  buf.length = 1;

  if (ioctl(device_fd_, VIDIOC_DQBUF, &buf) < 0) {
    if (errno == EAGAIN) {
      return WEBRTC_VIDEO_CODEC_OK;
    }
    RTC_LOG(LS_ERROR) << "Failed to dequeue output buffer: " << strerror(errno);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Process the encoded frame
  uint8_t* output_ptr = static_cast<uint8_t*>(output_buffers_[buf.index]);
  size_t output_size = buf.m.planes[0].bytesused;
  
  std::vector<uint8_t> packet(output_ptr, output_ptr + output_size);
  int32_t result = ProcessEncodedFrame(packet, input_frame);

  // Re-queue the output buffer
  memset(&buf, 0, sizeof(buf));
  memset(planes, 0, sizeof(planes));
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.index = buf.index;
  buf.m.planes = planes;
  buf.length = 1;
  
  if (ioctl(device_fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to re-queue output buffer";
  }

  frame_count_++;
  current_encoding_is_keyframe_ = false;
  return result;
}

int32_t V4L2H265EncoderImpl::ProcessEncodedFrame(
    std::vector<uint8_t>& packet,
    const VideoFrame& inputFrame) {
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.SetRtpTimestamp(inputFrame.rtp_timestamp());
  encoded_image_.SetSimulcastIndex(0);
  encoded_image_.ntp_time_ms_ = inputFrame.ntp_time_ms();
  encoded_image_.capture_time_ms_ = inputFrame.render_time_ms();
  encoded_image_.rotation_ = inputFrame.rotation();
  encoded_image_.content_type_ = VideoContentType::UNSPECIFIED;
  encoded_image_.timing_.flags = VideoSendTiming::kInvalid;
  encoded_image_._frameType =
      current_encoding_is_keyframe_ ? VideoFrameType::kVideoFrameKey
                                    : VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(inputFrame.color_space());

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(packet.data(), packet.size()));
  encoded_image_.set_size(packet.size());

  encoded_image_.qp_ = -1;

  CodecSpecificInfo codecInfo;
  codecInfo.codecType = kVideoCodecH265;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codecInfo);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode callback failed " << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo V4L2H265EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "V4L2 H265 Encoder (Jetson)";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void V4L2H265EncoderImpl::SetRates(
    const RateControlParameters& parameters) {
  if (!encoder_initialized_) {
    RTC_LOG(LS_WARNING) << "SetRates() while uninitialized.";
    return;
  }

  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);
  codec_.maxBitrate = parameters.bitrate.GetSpatialLayerSum(0);

  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  // Update V4L2 encoder settings
  if (device_fd_ >= 0) {
    struct v4l2_control ctrl;
    ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE;
    ctrl.value = configuration_.target_bps;
    if (ioctl(device_fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "Failed to update bitrate";
    }

    struct v4l2_streamparm parm;
    memset(&parm, 0, sizeof(parm));
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    parm.parm.output.timeperframe.numerator = 1;
    parm.parm.output.timeperframe.denominator = 
        static_cast<uint32_t>(parameters.framerate_fps);
    if (ioctl(device_fd_, VIDIOC_S_PARM, &parm) < 0) {
      RTC_LOG(LS_WARNING) << "Failed to update framerate";
    }
  }

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
  } else {
    configuration_.SetStreamState(false);
  }
}

void V4L2H265EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc

