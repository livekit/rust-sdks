#include "h264_encoder_impl.h"

#include <utility>

#include "api/video/i420_buffer.h"
#include "api/video/nv12_buffer.h"
#include "api/video/video_frame_buffer.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory_template.h"
#include "modules/video_coding/include/video_codec_interface.h"
#if defined(WEBRTC_USE_H264)
#include "api/video_codecs/video_encoder_factory_template_open_h264_adapter.h"
#endif
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"

#if defined(__linux__)
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>
#endif

namespace webrtc {

namespace {

using SoftwareFactory = webrtc::VideoEncoderFactoryTemplate<
#if defined(WEBRTC_USE_H264)
    webrtc::OpenH264EncoderTemplateAdapter
#endif
    >;

#if defined(__linux__)

static int xioctl(int fd, unsigned long request, void* arg) {
  int r;
  do {
    r = ioctl(fd, request, arg);
  } while (r == -1 && errno == EINTR);
  return r;
}

#endif  // __linux__

}  // namespace

V4L2H264EncoderImpl::V4L2H264EncoderImpl(const webrtc::Environment& env,
                                         const SdpVideoFormat& format)
    : env_(env), format_(format) {}

V4L2H264EncoderImpl::~V4L2H264EncoderImpl() {
#if defined(__linux__)
  CleanupV4L2();
#endif
}

int32_t V4L2H264EncoderImpl::InitEncode(const VideoCodec* codec_settings,
                                        const Settings& settings) {
  if (!codec_settings || codec_settings->codecType != kVideoCodecH264) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  width_ = codec_settings->width;
  height_ = codec_settings->height;

#if defined(__linux__)
  RTC_LOG(LS_INFO) << "V4L2H264EncoderImpl::InitEncode requested for H.264 "
                   << width_ << "x" << height_
                   << " maxFramerate=" << codec_settings->maxFramerate
                   << " maxBitrate=" << codec_settings->maxBitrate
                   << " minBitrate=" << codec_settings->minBitrate
                   << " startBitrate=" << codec_settings->startBitrate;

  if (InitV4L2Device(codec_settings) == WEBRTC_VIDEO_CODEC_OK) {
    v4l2_initialized_ = true;
    return WEBRTC_VIDEO_CODEC_OK;
  }
  RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: failed to initialize V4L2 "
                       "H.264 encoder; not falling back to software encoder";
  return WEBRTC_VIDEO_CODEC_ERROR;
#else
  // Software fallback is only used on non-Linux builds, where the V4L2 path is
  // not available.
  SoftwareFactory factory;
  fallback_encoder_ = factory.Create(env_, format_);
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->InitEncode(codec_settings, settings);
#endif
}

int32_t V4L2H264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
#if defined(__linux__)
  if (v4l2_initialized_) {
    encoded_image_callback_ = callback;
    return WEBRTC_VIDEO_CODEC_OK;
  }
#endif
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->RegisterEncodeCompleteCallback(callback);
}

int32_t V4L2H264EncoderImpl::Release() {
#if defined(__linux__)
  if (v4l2_initialized_) {
    CleanupV4L2();
    v4l2_initialized_ = false;
    return WEBRTC_VIDEO_CODEC_OK;
  }
#endif
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_OK;
  }
  return fallback_encoder_->Release();
}

int32_t V4L2H264EncoderImpl::Encode(
    const VideoFrame& frame,
    const std::vector<VideoFrameType>* frame_types) {
#if defined(__linux__)
  if (v4l2_initialized_) {
    return EncodeWithV4L2(frame, frame_types);
  }
#endif
  if (!fallback_encoder_) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return fallback_encoder_->Encode(frame, frame_types);
}

void V4L2H264EncoderImpl::SetRates(
    const RateControlParameters& rc_parameters) {
#if defined(__linux__)
  if (v4l2_initialized_) {
    // TODO: plumb V4L2 bitrate/framerate controls (e.g. V4L2_CID_MPEG_VIDEO_BITRATE).
    return;
  }
#endif
  if (!fallback_encoder_) {
    return;
  }
  fallback_encoder_->SetRates(rc_parameters);
}

VideoEncoder::EncoderInfo V4L2H264EncoderImpl::GetEncoderInfo() const {
  VideoEncoder::EncoderInfo info;
#if defined(__linux__)
  if (v4l2_initialized_) {
    info.implementation_name = "V4L2 H264 Encoder";
    info.is_hardware_accelerated = true;
    info.supports_native_handle = false;
    info.supports_simulcast = false;
    return info;
  }
#endif
  if (!fallback_encoder_) {
    info.implementation_name = "V4L2-H264 (uninitialized)";
    return info;
  }
  info = fallback_encoder_->GetEncoderInfo();
  info.implementation_name = "V4L2-H264 (software fallback)";
  return info;
}

#if defined(__linux__)

int V4L2H264EncoderImpl::InitV4L2Device(const VideoCodec* codec_settings) {
  // See NVIDIA Jetson V4L2 encoder docs:
  // https://docs.nvidia.com/jetson/l4t-multimedia/group__V4L2Enc.html
  // Default device node on Jetson Jetpack 6 is "/dev/v4l2-nvenc".
  // Older Jetpack versions may use "/dev/nvhost-msenc".
  // Allow override via LK_V4L2_ENCODER_DEVICE for flexibility.
  const char* dev_path = std::getenv("LK_V4L2_ENCODER_DEVICE");
  if (!dev_path) {
    dev_path = "/dev/v4l2-nvenc";
  }

  RTC_LOG(LS_WARNING) << "*** V4L2H264EncoderImpl: INITIALIZING V4L2 encoder at " << dev_path;

  fd_ = open(dev_path, O_RDWR | O_NONBLOCK, 0);
  if (fd_ < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: failed to open " << dev_path
                      << ": " << strerror(errno) << " (errno=" << errno << ")";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  RTC_LOG(LS_WARNING) << "*** V4L2H264EncoderImpl: Successfully opened encoder device: " << dev_path << " (fd=" << fd_ << ")";

  // Query capabilities so logs clearly show what the node supports.
  v4l2_capability caps = {};
  if (xioctl(fd_, VIDIOC_QUERYCAP, &caps) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: VIDIOC_QUERYCAP failed for "
                      << dev_path << ": " << strerror(errno);
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Format hex values as strings since RTC_LOG doesn't support std::hex manipulator
  char caps_hex[32], device_caps_hex[32];
  snprintf(caps_hex, sizeof(caps_hex), "0x%08x", caps.capabilities);
  snprintf(device_caps_hex, sizeof(device_caps_hex), "0x%08x", caps.device_caps);
  
  RTC_LOG(LS_INFO) << "V4L2H264EncoderImpl: opened device " << dev_path
                   << ", driver=\"" << reinterpret_cast<const char*>(caps.driver)
                   << "\", card=\"" << reinterpret_cast<const char*>(caps.card)
                   << "\", bus_info=\""
                   << reinterpret_cast<const char*>(caps.bus_info) << "\""
                   << ", capabilities=" << caps_hex
                   << ", device_caps=" << device_caps_hex;

  const bool has_m2m_mplane =
      (caps.device_caps & V4L2_CAP_VIDEO_M2M_MPLANE) != 0 ||
      (caps.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE) != 0;
  const bool has_capture_mplane =
      (caps.device_caps & V4L2_CAP_VIDEO_CAPTURE_MPLANE) != 0 ||
      (caps.capabilities & V4L2_CAP_VIDEO_CAPTURE_MPLANE) != 0;
  const bool has_output_mplane =
      (caps.device_caps & V4L2_CAP_VIDEO_OUTPUT_MPLANE) != 0 ||
      (caps.capabilities & V4L2_CAP_VIDEO_OUTPUT_MPLANE) != 0;

  if (!has_m2m_mplane && !(has_capture_mplane && has_output_mplane)) {
    RTC_LOG(LS_ERROR)
        << "V4L2H264EncoderImpl: device " << dev_path
        << " does not advertise VIDEO_M2M_MPLANE or separate "
           "CAPTURE_MPLANE/OUTPUT_MPLANE capabilities; cannot use as encoder.";
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // NOTE: Per NVIDIA docs, capture plane format must be set BEFORE output
  // plane format, and only then request buffers on any plane.
  // Configure CAPTURE queue: H.264 bitstream.
  v4l2_format fmt_cap = {};
  fmt_cap.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  fmt_cap.fmt.pix_mp.width = width_;
  fmt_cap.fmt.pix_mp.height = height_;
  fmt_cap.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264;
  fmt_cap.fmt.pix_mp.num_planes = 1;
  if (xioctl(fd_, VIDIOC_S_FMT, &fmt_cap) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: VIDIOC_S_FMT (CAPTURE) failed: "
                      << strerror(errno);
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Configure OUTPUT queue: YUV420M frames coming from WebRTC.
  // NVIDIA encoder supports V4L2_PIX_FMT_YUV420M on OUTPUT plane
  // ([V4L2Enc docs](https://docs.nvidia.com/jetson/l4t-multimedia/group__V4L2Enc.html)).
  v4l2_format fmt_out = {};
  fmt_out.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  fmt_out.fmt.pix_mp.width = width_;
  fmt_out.fmt.pix_mp.height = height_;
  fmt_out.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_YUV420M;
  fmt_out.fmt.pix_mp.num_planes = 3;
  if (xioctl(fd_, VIDIOC_S_FMT, &fmt_out) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: VIDIOC_S_FMT (OUTPUT) failed: "
                      << strerror(errno);
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Request and mmap OUTPUT buffers.
  v4l2_requestbuffers req_out = {};
  req_out.count = 4;
  req_out.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  req_out.memory = V4L2_MEMORY_MMAP;
  if (xioctl(fd_, VIDIOC_REQBUFS, &req_out) < 0 || req_out.count < 2) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: REQBUFS OUTPUT failed: "
                      << strerror(errno) << ", count=" << req_out.count;
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  output_buffers_.resize(req_out.count);
  for (uint32_t i = 0; i < req_out.count; ++i) {
    v4l2_buffer buf = {};
    v4l2_plane planes[3] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 3;
    buf.m.planes = planes;
    if (xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: QUERYBUF OUTPUT failed: "
                        << strerror(errno);
      CleanupV4L2();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    output_buffers_[i].planes.resize(3);
    for (uint32_t p = 0; p < 3; ++p) {
      void* start =
          mmap(nullptr, buf.m.planes[p].length, PROT_READ | PROT_WRITE,
               MAP_SHARED, fd_, buf.m.planes[p].m.mem_offset);
      if (start == MAP_FAILED) {
        RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: mmap OUTPUT failed: "
                          << strerror(errno);
        CleanupV4L2();
        return WEBRTC_VIDEO_CODEC_ERROR;
      }
      output_buffers_[i].planes[p].start = start;
      output_buffers_[i].planes[p].length = buf.m.planes[p].length;
    }
  }

  // Request and mmap CAPTURE buffers.
  v4l2_requestbuffers req_cap = {};
  req_cap.count = 4;
  req_cap.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  req_cap.memory = V4L2_MEMORY_MMAP;
  if (xioctl(fd_, VIDIOC_REQBUFS, &req_cap) < 0 || req_cap.count < 2) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: REQBUFS CAPTURE failed: "
                      << strerror(errno) << ", count=" << req_cap.count;
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  capture_buffers_.resize(req_cap.count);
  for (uint32_t i = 0; i < req_cap.count; ++i) {
    v4l2_buffer buf = {};
    v4l2_plane planes[1] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;
    if (xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: QUERYBUF CAPTURE failed: "
                        << strerror(errno);
      CleanupV4L2();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    capture_buffers_[i].planes.resize(1);
    void* start = mmap(nullptr, buf.m.planes[0].length, PROT_READ | PROT_WRITE,
                       MAP_SHARED, fd_, buf.m.planes[0].m.mem_offset);
    if (start == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: mmap CAPTURE failed: "
                        << strerror(errno);
      CleanupV4L2();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    capture_buffers_[i].planes[0].start = start;
    capture_buffers_[i].planes[0].length = buf.m.planes[0].length;

    // Enqueue all capture buffers.
    if (xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: QBUF CAPTURE failed: "
                        << strerror(errno);
      CleanupV4L2();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  // Start streaming on both queues.
  int type_out = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  if (xioctl(fd_, VIDIOC_STREAMON, &type_out) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: STREAMON OUTPUT failed: "
                      << strerror(errno);
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  int type_cap = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (xioctl(fd_, VIDIOC_STREAMON, &type_cap) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: STREAMON CAPTURE failed: "
                      << strerror(errno);
    CleanupV4L2();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  RTC_LOG(LS_WARNING) << "V4L2H264EncoderImpl: *** SUCCESSFULLY INITIALIZED V4L2 HARDWARE ENCODER *** "
                   << "device=" << dev_path << " resolution=" << width_ << "x" << height_ 
                   << " output_buffers=" << output_buffers_.size() 
                   << " capture_buffers=" << capture_buffers_.size();
  return WEBRTC_VIDEO_CODEC_OK;
}

int V4L2H264EncoderImpl::EncodeWithV4L2(
    const VideoFrame& frame,
    const std::vector<VideoFrameType>* frame_types) {
  // Log every 60 frames (~2 seconds at 30fps) to confirm V4L2 encoding is active
  static int encode_count = 0;
  if (++encode_count % 60 == 1) {
    RTC_LOG(LS_WARNING) << "*** V4L2H264EncoderImpl: Encoding with V4L2 hardware encoder (frame #" << encode_count << ")";
  }
  
  // For now we always send frames; keyframe control could be added via
  // encoder-specific controls.
  if (output_buffers_.empty() || capture_buffers_.empty()) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  rtc::scoped_refptr<VideoFrameBuffer> buffer = frame.video_frame_buffer();
  rtc::scoped_refptr<I420BufferInterface> i420 = buffer->ToI420();
  if (!i420) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: unable to get I420 buffer";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Get a free OUTPUT buffer by dequeuing; if none are queued yet, re-use index 0.
  v4l2_buffer buf_out = {};
  v4l2_plane planes_out[3] = {};
  buf_out.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf_out.memory = V4L2_MEMORY_MMAP;
  buf_out.length = 3;
  buf_out.m.planes = planes_out;

  if (xioctl(fd_, VIDIOC_DQBUF, &buf_out) < 0) {
    if (errno != EAGAIN) {
      RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: DQBUF OUTPUT failed: "
                        << strerror(errno);
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    // No queued output buffers yet; use index 0.
    buf_out.index = 0;
  }

  if (buf_out.index >= output_buffers_.size()) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: invalid OUTPUT buffer index";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Copy I420 planes into the three mmap'd YUV420M planes.
  auto& planes = output_buffers_[buf_out.index].planes;
  if (planes.size() < 3) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: OUTPUT buffer has "
                      << planes.size() << " planes, expected 3";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  const int y_stride_src = i420->StrideY();
  const int u_stride_src = i420->StrideU();
  const int v_stride_src = i420->StrideV();
  uint8_t* dst_y = static_cast<uint8_t*>(planes[0].start);
  uint8_t* dst_u = static_cast<uint8_t*>(planes[1].start);
  uint8_t* dst_v = static_cast<uint8_t*>(planes[2].start);

  // Y plane
  for (int y = 0; y < static_cast<int>(height_); ++y) {
    memcpy(dst_y + y * width_, i420->DataY() + y * y_stride_src, width_);
  }
  // U/V planes (width/2 x height/2)
  const int cw = width_ / 2;
  const int ch = height_ / 2;
  for (int y = 0; y < ch; ++y) {
    memcpy(dst_u + y * cw, i420->DataU() + y * u_stride_src, cw);
    memcpy(dst_v + y * cw, i420->DataV() + y * v_stride_src, cw);
  }

  planes_out[0].bytesused = width_ * height_;
  planes_out[1].bytesused = cw * ch;
  planes_out[2].bytesused = cw * ch;

  if (xioctl(fd_, VIDIOC_QBUF, &buf_out) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: QBUF OUTPUT failed: "
                      << strerror(errno);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Dequeue one encoded frame from CAPTURE and deliver via callback.
  EncodedImage encoded_image;
  int drain_res = DrainEncodedFrame(encoded_image);
  if (drain_res != WEBRTC_VIDEO_CODEC_OK) {
    return drain_res;
  }

  encoded_image.SetRtpTimestamp(frame.rtp_timestamp());
  encoded_image.SetColorSpace(frame.color_space());
  encoded_image._encodedWidth = width_;
  encoded_image._encodedHeight = height_;
  encoded_image._frameType = VideoFrameType::kVideoFrameDelta;

  CodecSpecificInfo codec_specific;
  codec_specific.codecType = kVideoCodecH264;

  if (!encoded_image_callback_) {
    return WEBRTC_VIDEO_CODEC_OK;
  }
  auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image, &codec_specific);
  if (result.error != EncodedImageCallback::Result::OK) {
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int V4L2H264EncoderImpl::DrainEncodedFrame(EncodedImage& encoded_image) {
  v4l2_buffer buf_cap = {};
  v4l2_plane planes_cap[1] = {};
  buf_cap.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf_cap.memory = V4L2_MEMORY_MMAP;
  buf_cap.length = 1;
  buf_cap.m.planes = planes_cap;

  if (xioctl(fd_, VIDIOC_DQBUF, &buf_cap) < 0) {
    if (errno == EAGAIN) {
      // No encoded data yet; treat as no output for this frame.
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: DQBUF CAPTURE failed: "
                      << strerror(errno);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (buf_cap.index >= capture_buffers_.size()) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: invalid CAPTURE buffer index";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  auto& planes = capture_buffers_[buf_cap.index].planes;
  if (planes.empty()) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: CAPTURE buffer has no planes";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  uint8_t* src = static_cast<uint8_t*>(planes[0].start);
  size_t bytes_used = buf_cap.m.planes[0].bytesused;
  auto buffer = EncodedImageBuffer::Create(bytes_used);
  memcpy(buffer->data(), src, bytes_used);

  encoded_image.SetEncodedData(std::move(buffer));

  // Re-queue the capture buffer for future use.
  if (xioctl(fd_, VIDIOC_QBUF, &buf_cap) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2H264EncoderImpl: re-QBUF CAPTURE failed: "
                      << strerror(errno);
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

void V4L2H264EncoderImpl::CleanupV4L2() {
  if (fd_ >= 0) {
    int type_out = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    int type_cap = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    xioctl(fd_, VIDIOC_STREAMOFF, &type_out);
    xioctl(fd_, VIDIOC_STREAMOFF, &type_cap);
  }

  for (auto& buf : output_buffers_) {
    for (auto& plane : buf.planes) {
      if (plane.start && plane.length) {
        munmap(plane.start, plane.length);
      }
    }
  }
  output_buffers_.clear();

  for (auto& buf : capture_buffers_) {
    for (auto& plane : buf.planes) {
      if (plane.start && plane.length) {
        munmap(plane.start, plane.length);
      }
    }
  }
  capture_buffers_.clear();

  if (fd_ >= 0) {
    close(fd_);
    fd_ = -1;
  }
}

#endif  // __linux__

}  // namespace webrtc


