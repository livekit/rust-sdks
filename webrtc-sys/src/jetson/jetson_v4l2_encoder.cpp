#include "jetson_v4l2_encoder.h"

#include <dirent.h>
#include <fcntl.h>
#include <poll.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <limits>

#include "rtc_base/logging.h"

namespace {

constexpr int kDefaultBufferCount = 4;
constexpr int kPollTimeoutMs = 2000;

bool IoctlWithRetry(int fd, unsigned long request, void* arg) {
  int result = 0;
  do {
    result = ioctl(fd, request, arg);
  } while (result == -1 && errno == EINTR);
  return result != -1;
}

uint32_t CodecToV4L2PixFmt(livekit::JetsonCodec codec) {
  return codec == livekit::JetsonCodec::kH264 ? V4L2_PIX_FMT_H264
                                              : V4L2_PIX_FMT_HEVC;
}

}  // namespace

namespace livekit {

JetsonV4L2Encoder::JetsonV4L2Encoder(JetsonCodec codec) : codec_(codec) {}

JetsonV4L2Encoder::~JetsonV4L2Encoder() {
  Destroy();
}

bool JetsonV4L2Encoder::IsSupported() {
  return IsCodecSupported(JetsonCodec::kH264) ||
         IsCodecSupported(JetsonCodec::kH265);
}

bool JetsonV4L2Encoder::IsCodecSupported(JetsonCodec codec) {
  return FindEncoderDevice(codec).has_value();
}

std::optional<std::string> JetsonV4L2Encoder::FindEncoderDevice(
    JetsonCodec codec) {
  DIR* dir = opendir("/dev");
  if (!dir) {
    return std::nullopt;
  }

  struct dirent* entry = nullptr;
  while ((entry = readdir(dir)) != nullptr) {
    if (strncmp(entry->d_name, "video", 5) != 0) {
      continue;
    }
    std::string path = std::string("/dev/") + entry->d_name;
    int fd = open(path.c_str(), O_RDWR | O_NONBLOCK);
    if (fd < 0) {
      continue;
    }

    v4l2_capability caps = {};
    bool ok = IoctlWithRetry(fd, VIDIOC_QUERYCAP, &caps);
    if (ok && (caps.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE) &&
        (caps.capabilities & V4L2_CAP_STREAMING) &&
        DeviceSupportsCodec(fd, codec)) {
      close(fd);
      closedir(dir);
      return path;
    }

    close(fd);
  }

  closedir(dir);
  return std::nullopt;
}

bool JetsonV4L2Encoder::DeviceSupportsCodec(int fd, JetsonCodec codec) {
  v4l2_fmtdesc desc = {};
  desc.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  for (desc.index = 0; IoctlWithRetry(fd, VIDIOC_ENUM_FMT, &desc);
       ++desc.index) {
    if (desc.pixelformat == CodecToV4L2PixFmt(codec)) {
      return true;
    }
  }
  return false;
}

bool JetsonV4L2Encoder::Initialize(int width,
                                   int height,
                                   int framerate,
                                   int bitrate_bps,
                                   int keyframe_interval) {
  if (initialized_) {
    return true;
  }

  width_ = width;
  height_ = height;
  framerate_ = framerate;
  bitrate_bps_ = bitrate_bps;
  keyframe_interval_ = keyframe_interval;

  auto device = FindEncoderDevice(codec_);
  if (!device.has_value()) {
    RTC_LOG(LS_WARNING) << "Jetson V4L2 encoder device not found.";
    return false;
  }
  device_path_ = *device;

  if (!OpenDevice()) {
    return false;
  }
  if (!ConfigureFormats()) {
    Destroy();
    return false;
  }
  if (!ConfigureControls()) {
    Destroy();
    return false;
  }
  if (!SetupBuffers()) {
    Destroy();
    return false;
  }
  if (!QueueCaptureBuffers()) {
    Destroy();
    return false;
  }
  if (!StartStreaming()) {
    Destroy();
    return false;
  }

  initialized_ = true;
  return true;
}

void JetsonV4L2Encoder::Destroy() {
  StopStreaming();

  for (auto& buffer : output_buffers_) {
    for (auto& plane : buffer.planes) {
      if (plane.start && plane.length) {
        munmap(plane.start, plane.length);
      }
    }
  }
  for (auto& buffer : capture_buffers_) {
    for (auto& plane : buffer.planes) {
      if (plane.start && plane.length) {
        munmap(plane.start, plane.length);
      }
    }
  }

  output_buffers_.clear();
  capture_buffers_.clear();

  if (fd_ >= 0) {
    close(fd_);
  }

  fd_ = -1;
  initialized_ = false;
  streaming_ = false;
}

bool JetsonV4L2Encoder::IsInitialized() const {
  return initialized_;
}

bool JetsonV4L2Encoder::Encode(const uint8_t* src_y,
                               int stride_y,
                               const uint8_t* src_uv,
                               int stride_uv,
                               bool force_keyframe,
                               std::vector<uint8_t>* encoded,
                               bool* is_keyframe) {
  if (!initialized_) {
    return false;
  }

  if (force_keyframe) {
    SetControl(V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME, 1);
  }

  if (!QueueOutputBuffer(next_output_index_, src_y, stride_y, src_uv,
                         stride_uv)) {
    return false;
  }
  next_output_index_ =
      (next_output_index_ + 1) %
      (output_buffer_count_ > 0 ? output_buffer_count_ : 1);

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    return false;
  }

  DequeueOutputBuffer();
  return true;
}

void JetsonV4L2Encoder::SetRates(int framerate, int bitrate_bps) {
  framerate_ = framerate;
  bitrate_bps_ = bitrate_bps;

  SetStreamParam(framerate_);
  SetControl(V4L2_CID_MPEG_VIDEO_BITRATE, bitrate_bps_);
}

void JetsonV4L2Encoder::SetKeyframeInterval(int keyframe_interval) {
  keyframe_interval_ = keyframe_interval;
  SetControl(V4L2_CID_MPEG_VIDEO_GOP_SIZE, keyframe_interval_);
}

bool JetsonV4L2Encoder::OpenDevice() {
  fd_ = open(device_path_.c_str(), O_RDWR | O_NONBLOCK);
  if (fd_ < 0) {
    RTC_LOG(LS_ERROR) << "Failed to open V4L2 encoder device: " << device_path_;
    return false;
  }
  return true;
}

bool JetsonV4L2Encoder::ConfigureFormats() {
  v4l2_format output_format = {};
  output_format.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  output_format.fmt.pix_mp.width = width_;
  output_format.fmt.pix_mp.height = height_;
  output_format.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_NV12M;
  output_format.fmt.pix_mp.num_planes = 2;
  output_format.fmt.pix_mp.plane_fmt[0].bytesperline = width_;
  output_format.fmt.pix_mp.plane_fmt[0].sizeimage = width_ * height_;
  output_format.fmt.pix_mp.plane_fmt[1].bytesperline = width_;
  output_format.fmt.pix_mp.plane_fmt[1].sizeimage = width_ * height_ / 2;

  if (!IoctlWithRetry(fd_, VIDIOC_S_FMT, &output_format)) {
    RTC_LOG(LS_ERROR) << "Failed to set V4L2 output format.";
    return false;
  }

  v4l2_format capture_format = {};
  capture_format.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  capture_format.fmt.pix_mp.width = width_;
  capture_format.fmt.pix_mp.height = height_;
  capture_format.fmt.pix_mp.pixelformat = CodecToV4L2PixFmt(codec_);
  capture_format.fmt.pix_mp.num_planes = 1;
  capture_format.fmt.pix_mp.plane_fmt[0].sizeimage = width_ * height_ * 2;

  if (!IoctlWithRetry(fd_, VIDIOC_S_FMT, &capture_format)) {
    RTC_LOG(LS_ERROR) << "Failed to set V4L2 capture format.";
    return false;
  }

  return true;
}

bool JetsonV4L2Encoder::ConfigureControls() {
  if (!SetStreamParam(framerate_)) {
    RTC_LOG(LS_WARNING) << "Failed to set V4L2 framerate.";
  }
  if (!SetControl(V4L2_CID_MPEG_VIDEO_BITRATE, bitrate_bps_)) {
    RTC_LOG(LS_WARNING) << "Failed to set V4L2 bitrate.";
  }
  if (keyframe_interval_ > 0) {
    if (!SetControl(V4L2_CID_MPEG_VIDEO_GOP_SIZE, keyframe_interval_)) {
      RTC_LOG(LS_WARNING) << "Failed to set V4L2 GOP size.";
    }
  }
  return true;
}

bool JetsonV4L2Encoder::SetupBuffers() {
  v4l2_requestbuffers req = {};
  req.count = kDefaultBufferCount;
  req.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  req.memory = V4L2_MEMORY_MMAP;
  if (!IoctlWithRetry(fd_, VIDIOC_REQBUFS, &req)) {
    RTC_LOG(LS_ERROR) << "Failed to request output buffers.";
    return false;
  }
  output_buffer_count_ = req.count;
  output_buffers_.resize(output_buffer_count_);

  for (int i = 0; i < output_buffer_count_; ++i) {
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 2;
    buf.m.planes = planes;

    if (!IoctlWithRetry(fd_, VIDIOC_QUERYBUF, &buf)) {
      RTC_LOG(LS_ERROR) << "Failed to query output buffer.";
      return false;
    }

    output_buffers_[i].planes.resize(2);
    for (size_t plane = 0; plane < 2; ++plane) {
      void* start = mmap(nullptr, buf.m.planes[plane].length, PROT_READ | PROT_WRITE,
                         MAP_SHARED, fd_, buf.m.planes[plane].m.mem_offset);
      if (start == MAP_FAILED) {
        RTC_LOG(LS_ERROR) << "Failed to mmap output buffer plane.";
        return false;
      }
      output_buffers_[i].planes[plane].start = start;
      output_buffers_[i].planes[plane].length = buf.m.planes[plane].length;
    }
  }

  req = {};
  req.count = kDefaultBufferCount;
  req.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  req.memory = V4L2_MEMORY_MMAP;
  if (!IoctlWithRetry(fd_, VIDIOC_REQBUFS, &req)) {
    RTC_LOG(LS_ERROR) << "Failed to request capture buffers.";
    return false;
  }
  capture_buffer_count_ = req.count;
  capture_buffers_.resize(capture_buffer_count_);

  for (int i = 0; i < capture_buffer_count_; ++i) {
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;

    if (!IoctlWithRetry(fd_, VIDIOC_QUERYBUF, &buf)) {
      RTC_LOG(LS_ERROR) << "Failed to query capture buffer.";
      return false;
    }

    capture_buffers_[i].planes.resize(1);
    void* start = mmap(nullptr, buf.m.planes[0].length, PROT_READ | PROT_WRITE,
                       MAP_SHARED, fd_, buf.m.planes[0].m.mem_offset);
    if (start == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "Failed to mmap capture buffer.";
      return false;
    }
    capture_buffers_[i].planes[0].start = start;
    capture_buffers_[i].planes[0].length = buf.m.planes[0].length;
  }

  return true;
}

bool JetsonV4L2Encoder::QueueCaptureBuffers() {
  for (int i = 0; i < capture_buffer_count_; ++i) {
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;

    if (!IoctlWithRetry(fd_, VIDIOC_QBUF, &buf)) {
      RTC_LOG(LS_ERROR) << "Failed to queue capture buffer.";
      return false;
    }
  }
  return true;
}

bool JetsonV4L2Encoder::StartStreaming() {
  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  if (!IoctlWithRetry(fd_, VIDIOC_STREAMON, &type)) {
    RTC_LOG(LS_ERROR) << "Failed to stream on output.";
    return false;
  }
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (!IoctlWithRetry(fd_, VIDIOC_STREAMON, &type)) {
    RTC_LOG(LS_ERROR) << "Failed to stream on capture.";
    return false;
  }
  streaming_ = true;
  return true;
}

void JetsonV4L2Encoder::StopStreaming() {
  if (!streaming_) {
    return;
  }
  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  IoctlWithRetry(fd_, VIDIOC_STREAMOFF, &type);
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  IoctlWithRetry(fd_, VIDIOC_STREAMOFF, &type);
  streaming_ = false;
}

bool JetsonV4L2Encoder::QueueOutputBuffer(int index,
                                          const uint8_t* src_y,
                                          int stride_y,
                                          const uint8_t* src_uv,
                                          int stride_uv) {
  if (index < 0 || index >= output_buffer_count_) {
    return false;
  }

  auto& buffer = output_buffers_[index];
  if (buffer.planes.size() < 2) {
    return false;
  }

  uint8_t* dst_y = static_cast<uint8_t*>(buffer.planes[0].start);
  uint8_t* dst_uv = static_cast<uint8_t*>(buffer.planes[1].start);

  for (int row = 0; row < height_; ++row) {
    std::memcpy(dst_y + row * width_, src_y + row * stride_y, width_);
  }
  int uv_height = height_ / 2;
  for (int row = 0; row < uv_height; ++row) {
    std::memcpy(dst_uv + row * width_, src_uv + row * stride_uv, width_);
  }

  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.index = index;
  buf.length = 2;
  buf.m.planes = planes;
  buf.m.planes[0].bytesused = width_ * height_;
  buf.m.planes[1].bytesused = width_ * height_ / 2;
  buf.m.planes[0].length = buffer.planes[0].length;
  buf.m.planes[1].length = buffer.planes[1].length;

  if (!IoctlWithRetry(fd_, VIDIOC_QBUF, &buf)) {
    RTC_LOG(LS_ERROR) << "Failed to queue output buffer.";
    return false;
  }

  return true;
}

bool JetsonV4L2Encoder::DequeueCaptureBuffer(std::vector<uint8_t>* encoded,
                                             bool* is_keyframe) {
  pollfd pfd = {};
  pfd.fd = fd_;
  pfd.events = POLLIN;
  int poll_result = poll(&pfd, 1, kPollTimeoutMs);
  if (poll_result <= 0) {
    RTC_LOG(LS_ERROR) << "Timed out waiting for encoded frame.";
    return false;
  }

  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 1;
  buf.m.planes = planes;

  if (!IoctlWithRetry(fd_, VIDIOC_DQBUF, &buf)) {
    RTC_LOG(LS_ERROR) << "Failed to dequeue capture buffer.";
    return false;
  }

  if (is_keyframe) {
    *is_keyframe = (buf.flags & V4L2_BUF_FLAG_KEYFRAME) != 0;
  }

  size_t bytes_used = buf.m.planes[0].bytesused;
  encoded->resize(bytes_used);
  std::memcpy(encoded->data(), capture_buffers_[buf.index].planes[0].start,
              bytes_used);

  if (!IoctlWithRetry(fd_, VIDIOC_QBUF, &buf)) {
    RTC_LOG(LS_ERROR) << "Failed to re-queue capture buffer.";
    return false;
  }

  return true;
}

void JetsonV4L2Encoder::DequeueOutputBuffer() {
  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 2;
  buf.m.planes = planes;

  IoctlWithRetry(fd_, VIDIOC_DQBUF, &buf);
}

bool JetsonV4L2Encoder::SetControl(uint32_t id, int32_t value) {
  v4l2_control ctrl = {};
  ctrl.id = id;
  ctrl.value = value;
  return IoctlWithRetry(fd_, VIDIOC_S_CTRL, &ctrl);
}

bool JetsonV4L2Encoder::SetStreamParam(int framerate) {
  if (framerate <= 0) {
    return false;
  }

  v4l2_streamparm parm = {};
  parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  parm.parm.output.timeperframe.numerator = 1;
  parm.parm.output.timeperframe.denominator = framerate;
  return IoctlWithRetry(fd_, VIDIOC_S_PARM, &parm);
}

}  // namespace livekit
