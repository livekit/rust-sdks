#include "v4l2_h264_encoder.h"

#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <algorithm>

namespace livekit {

namespace {
bool xioctl(int fd, unsigned long req, void* arg) {
  while (true) {
    int r = ioctl(fd, req, arg);
    if (r == -1 && errno == EINTR) continue;
    return r != -1;
  }
}
}  // namespace

V4L2H264Encoder::V4L2H264Encoder() {}
V4L2H264Encoder::~V4L2H264Encoder() { Shutdown(); }

bool V4L2H264Encoder::OpenDevice() {
  // Probe a small range of nodes.
  char path[32];
  for (int i = 0; i < 16; ++i) {
    snprintf(path, sizeof(path), "/dev/video%d", i);
    int fd = open(path, O_RDWR | O_NONBLOCK);
    if (fd < 0) continue;
    v4l2_capability cap{};
    if (!xioctl(fd, VIDIOC_QUERYCAP, &cap)) {
      close(fd);
      continue;
    }
    uint32_t dev_caps = (cap.capabilities & V4L2_CAP_DEVICE_CAPS) ? cap.device_caps : cap.capabilities;
    bool is_m2m = (dev_caps & V4L2_CAP_VIDEO_M2M_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_M2M);
    bool has_output = (dev_caps & V4L2_CAP_VIDEO_OUTPUT_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_OUTPUT);
    bool has_capture = (dev_caps & V4L2_CAP_VIDEO_CAPTURE_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_CAPTURE);
    if (is_m2m && has_output && has_capture) {
      fd_ = fd;
      return true;
    }
    close(fd);
  }
  return false;
}

bool V4L2H264Encoder::SetupOutputFormat(int width, int height) {
  v4l2_format fmt{};
  fmt.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  fmt.fmt.pix_mp.width = width;
  fmt.fmt.pix_mp.height = height;
  fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_NV12M;  // multi-plane NV12
  fmt.fmt.pix_mp.num_planes = 2;
  return xioctl(fd_, VIDIOC_S_FMT, &fmt);
}

bool V4L2H264Encoder::SetupCaptureFormat() {
  v4l2_format fmt{};
  fmt.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264;
  fmt.fmt.pix_mp.width = width_;
  fmt.fmt.pix_mp.height = height_;
  // Let the driver decide buffer sizes.
  return xioctl(fd_, VIDIOC_S_FMT, &fmt);
}

bool V4L2H264Encoder::SetControls(int fps, int bitrate_bps) {
  // Bitrate (if supported)
  v4l2_control ctrl{};
  ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE;
  ctrl.value = bitrate_bps;
  xioctl(fd_, VIDIOC_S_CTRL, &ctrl);

  // Framerate: try setting timeperframe on OUTPUT streamparm
  v4l2_streamparm parm{};
  parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  parm.parm.output.timeperframe.numerator = 1;
  parm.parm.output.timeperframe.denominator = std::max(1, fps);
  xioctl(fd_, VIDIOC_S_PARM, &parm);

  return true;  // best effort
}

bool V4L2H264Encoder::RequestBuffers() {
  // OUTPUT: DMABUF, need slots
  {
    v4l2_requestbuffers req{};
    req.count = 4;
    req.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    req.memory = V4L2_MEMORY_DMABUF;
    if (!xioctl(fd_, VIDIOC_REQBUFS, &req) || req.count < 1) {
      return false;
    }
  }
  // CAPTURE: MMAP
  {
    v4l2_requestbuffers req{};
    req.count = 4;
    req.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    req.memory = V4L2_MEMORY_MMAP;
    if (!xioctl(fd_, VIDIOC_REQBUFS, &req) || req.count < 1) {
      return false;
    }
    capture_buffers_.resize(req.count);
    for (uint32_t i = 0; i < req.count; ++i) {
      v4l2_buffer buf{};
      v4l2_plane planes[1]{};  // usually single plane for H264 capture
      buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
      buf.memory = V4L2_MEMORY_MMAP;
      buf.index = i;
      buf.length = 1;
      buf.m.planes = planes;
      if (!xioctl(fd_, VIDIOC_QUERYBUF, &buf)) {
        return false;
      }
      void* addr = mmap(nullptr, buf.m.planes[0].length, PROT_READ | PROT_WRITE, MAP_SHARED, fd_, buf.m.planes[0].m.mem_offset);
      if (addr == MAP_FAILED) return false;
      capture_buffers_[i] = {addr, buf.m.planes[0].length};
      // Queue initially
      if (!xioctl(fd_, VIDIOC_QBUF, &buf)) return false;
    }
  }
  return true;
}

bool V4L2H264Encoder::StartStreaming() {
  v4l2_buf_type type;
  type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  if (!xioctl(fd_, VIDIOC_STREAMON, &type)) return false;
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (!xioctl(fd_, VIDIOC_STREAMON, &type)) return false;
  streaming_ = true;
  return true;
}

void V4L2H264Encoder::StopStreaming() {
  if (!streaming_) return;
  v4l2_buf_type type;
  type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  xioctl(fd_, VIDIOC_STREAMOFF, &type);
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  xioctl(fd_, VIDIOC_STREAMOFF, &type);
  streaming_ = false;
}

bool V4L2H264Encoder::Initialize(int width, int height, int fps, int bitrate_bps) {
  width_ = width;
  height_ = height;
  fps_ = fps;
  bitrate_bps_ = bitrate_bps;
  if (!OpenDevice()) return false;
  if (!SetupOutputFormat(width, height)) return false;
  if (!SetupCaptureFormat()) return false;
  if (!SetControls(fps, bitrate_bps)) return false;
  if (!RequestBuffers()) return false;
  if (!StartStreaming()) return false;
  return true;
}

bool V4L2H264Encoder::QueueOutput(const DmabufPlanesNV12& planes) {
  v4l2_buffer buf{};
  v4l2_plane v4l2_planes[2]{};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_DMABUF;
  buf.length = 2;
  buf.m.planes = v4l2_planes;
  // Let driver pick buffer slot (or reuse 0); with DMABUF, index still required; use 0 and reuse.
  buf.index = 0;

  // Plane 0: Y
  v4l2_planes[0].m.fd = planes.fd_y;
  v4l2_planes[0].bytesused = planes.stride_y * planes.height;
  v4l2_planes[0].length = v4l2_planes[0].bytesused;

  // Plane 1: UV
  v4l2_planes[1].m.fd = planes.fd_uv;
  v4l2_planes[1].bytesused = planes.stride_uv * ((planes.height + 1) / 2);
  v4l2_planes[1].length = v4l2_planes[1].bytesused;

  return xioctl(fd_, VIDIOC_QBUF, &buf);
}

bool V4L2H264Encoder::DequeueOutput() {
  v4l2_buffer buf{};
  v4l2_plane planes[2]{};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_DMABUF;
  buf.length = 2;
  buf.m.planes = planes;
  if (!xioctl(fd_, VIDIOC_DQBUF, &buf)) {
    if (errno == EAGAIN) return true;  // nothing to dequeue, ok
    return false;
  }
  return true;
}

std::optional<std::pair<int, size_t>> V4L2H264Encoder::DequeueCaptureIndexAndSize() {
  v4l2_buffer buf{};
  v4l2_plane planes[1]{};
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 1;
  buf.m.planes = planes;
  if (!xioctl(fd_, VIDIOC_DQBUF, &buf)) {
    if (errno == EAGAIN) return std::nullopt;
    return std::nullopt;
  }
  int index = buf.index;
  size_t size = buf.m.planes[0].bytesused;
  return std::make_pair(index, size);
}

bool V4L2H264Encoder::QueueCapture(int index) {
  v4l2_buffer buf{};
  v4l2_plane planes[1]{};
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.index = (uint32_t)index;
  buf.length = 1;
  buf.m.planes = planes;
  return xioctl(fd_, VIDIOC_QBUF, &buf);
}

bool V4L2H264Encoder::EnqueueDmabufFrame(const DmabufPlanesNV12& planes, bool keyframe) {
  if (keyframe) {
    v4l2_control ctrl{};
    ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
    ctrl.value = 1;
    xioctl(fd_, VIDIOC_S_CTRL, &ctrl);
  }
  if (!QueueOutput(planes)) return false;
  // Some drivers require OUTPUT DQBUF; drain non-blocking.
  DequeueOutput();
  return true;
}

std::optional<std::vector<uint8_t>> V4L2H264Encoder::DequeueEncoded() {
  auto idx_size = DequeueCaptureIndexAndSize();
  if (!idx_size.has_value()) return std::nullopt;
  int index = idx_size->first;
  size_t size = idx_size->second;
  if (index < 0 || (size_t)index >= capture_buffers_.size()) {
    return std::nullopt;
  }
  std::vector<uint8_t> out;
  out.resize(size);
  if (size > 0) {
    memcpy(out.data(), capture_buffers_[index].addr, size);
  }
  // Re-queue capture buffer
  QueueCapture(index);
  return out;
}

void V4L2H264Encoder::UpdateRates(int fps, int bitrate_bps) {
  fps_ = fps;
  bitrate_bps_ = bitrate_bps;
  SetControls(fps, bitrate_bps);
}

void V4L2H264Encoder::Shutdown() {
  StopStreaming();
  for (auto& b : capture_buffers_) {
    if (b.addr != MAP_FAILED && b.length > 0) {
      munmap(b.addr, b.length);
    }
  }
  capture_buffers_.clear();
  if (fd_ >= 0) {
    close(fd_);
    fd_ = -1;
  }
}

}  // namespace livekit


