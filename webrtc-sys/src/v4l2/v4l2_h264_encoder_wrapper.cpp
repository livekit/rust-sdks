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

#include "v4l2_h264_encoder_wrapper.h"

#include <dirent.h>
#include <fcntl.h>
#include <poll.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

#include <linux/videodev2.h>

#include <algorithm>
#include <string>
#include <vector>

#include "rtc_base/logging.h"

namespace livekit_ffi {

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

int V4l2H264EncoderWrapper::Xioctl(int fd, unsigned long ctl, void* arg) {
  int ret;
  int tries = 10;
  do {
    ret = ioctl(fd, ctl, arg);
  } while (ret == -1 && errno == EINTR && tries-- > 0);
  return ret;
}

// Compute the byte size of a planar frame for the given fourcc.
static int FrameSizeForFourcc(uint32_t fourcc, int width, int height) {
  // YUV420 and NV12 are both 12 bpp (1.5 bytes/pixel).
  if (fourcc == V4L2_PIX_FMT_YUV420 || fourcc == V4L2_PIX_FMT_NV12) {
    return width * height * 3 / 2;
  }
  // Conservative fallback: treat as 1 byte/pixel.
  return width * height;
}

// Set a single V4L2 control, logging a warning on failure but not aborting.
static void TrySetControl(int fd, uint32_t id, int32_t value, const char* name) {
  v4l2_control ctrl = {};
  ctrl.id = id;
  ctrl.value = value;
  if (V4l2H264EncoderWrapper::Xioctl(fd, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "V4L2: Failed to set " << name << ": "
                        << strerror(errno);
  }
}

static uint32_t V4l2MemoryFor(OutputBufferMode mode) {
  switch (mode) {
    case OutputBufferMode::Mmap:
      return V4L2_MEMORY_MMAP;
    case OutputBufferMode::UserPtr:
      return V4L2_MEMORY_USERPTR;
    case OutputBufferMode::Dmabuf:
      return V4L2_MEMORY_DMABUF;
  }
  return V4L2_MEMORY_MMAP;
}

static const char* ModeName(OutputBufferMode mode) {
  switch (mode) {
    case OutputBufferMode::Mmap:
      return "MMAP";
    case OutputBufferMode::UserPtr:
      return "USERPTR";
    case OutputBufferMode::Dmabuf:
      return "DMABUF";
  }
  return "?";
}

// ---------------------------------------------------------------------------
// Construction / destruction
// ---------------------------------------------------------------------------

V4l2H264EncoderWrapper::V4l2H264EncoderWrapper() = default;

V4l2H264EncoderWrapper::~V4l2H264EncoderWrapper() {
  if (initialized_) {
    Destroy();
  }
}

// ---------------------------------------------------------------------------
// Device discovery
// ---------------------------------------------------------------------------

std::string V4l2H264EncoderWrapper::FindEncoderDevice() {
  DIR* dir = opendir("/dev");
  if (!dir) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to open /dev";
    return "";
  }

  std::string result;
  struct dirent* entry;
  while ((entry = readdir(dir)) != nullptr) {
    std::string name(entry->d_name);
    if (name.find("video") != 0)
      continue;

    std::string path = "/dev/" + name;
    int fd = open(path.c_str(), O_RDWR | O_NONBLOCK, 0);
    if (fd < 0)
      continue;

    // Query device capabilities.
    struct v4l2_capability cap = {};
    if (Xioctl(fd, VIDIOC_QUERYCAP, &cap) < 0) {
      close(fd);
      continue;
    }

    // We need an M2M device with multi-planar support. Some drivers
    // advertise the flag in |capabilities|, others in |device_caps|.
    bool is_m2m = (cap.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE) ||
                  (cap.device_caps & V4L2_CAP_VIDEO_M2M_MPLANE);
    if (!is_m2m) {
      close(fd);
      continue;
    }

    // Enumerate CAPTURE formats looking for H.264.
    struct v4l2_fmtdesc fmtdesc = {};
    fmtdesc.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    bool supports_h264 = false;
    while (Xioctl(fd, VIDIOC_ENUM_FMT, &fmtdesc) == 0) {
      if (fmtdesc.pixelformat == V4L2_PIX_FMT_H264) {
        supports_h264 = true;
        break;
      }
      fmtdesc.index++;
    }

    close(fd);

    if (supports_h264) {
      RTC_LOG(LS_INFO) << "V4L2: Found H.264 M2M encoder at " << path
                       << " (" << cap.card << ")";
      result = path;
      break;
    }
  }

  closedir(dir);
  return result;
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

bool V4l2H264EncoderWrapper::Initialize(int width,
                                       int height,
                                       int bitrate,
                                       int keyframe_interval,
                                       int framerate,
                                       OutputBufferMode mode,
                                       uint32_t input_fourcc,
                                       const std::string& device_path) {
  if (initialized_)
    Destroy();

  width_ = width;
  height_ = height;
  framerate_ = framerate > 0 ? framerate : 30;
  mode_ = mode;
  input_fourcc_ = input_fourcc;
  frame_size_ = FrameSizeForFourcc(input_fourcc, width, height);

  // --- Open the encoder device ---

  std::string path = device_path;
  if (path.empty())
    path = FindEncoderDevice();
  if (path.empty()) {
    RTC_LOG(LS_ERROR) << "V4L2: No H.264 M2M encoder device found";
    return false;
  }

  fd_ = open(path.c_str(), O_RDWR, 0);
  if (fd_ < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to open " << path
                      << ": " << strerror(errno);
    return false;
  }
  RTC_LOG(LS_INFO) << "V4L2: Opened encoder device " << path
                   << " (fd " << fd_ << ", mode " << ModeName(mode_) << ")";

  // --- Set OUTPUT format (raw YUV fed into the encoder) ---
  //
  // Format must be negotiated BEFORE codec-specific controls are set --
  // some V4L2 drivers (including bcm2835-codec) only expose H.264 controls
  // after the CAPTURE format selects V4L2_PIX_FMT_H264.

  v4l2_format fmt = {};
  fmt.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  fmt.fmt.pix_mp.width = width;
  fmt.fmt.pix_mp.height = height;
  fmt.fmt.pix_mp.pixelformat = input_fourcc_;
  fmt.fmt.pix_mp.field = V4L2_FIELD_ANY;
  fmt.fmt.pix_mp.colorspace = V4L2_COLORSPACE_SMPTE170M;
  fmt.fmt.pix_mp.num_planes = 1;
  fmt.fmt.pix_mp.plane_fmt[0].bytesperline = width;
  fmt.fmt.pix_mp.plane_fmt[0].sizeimage = frame_size_;
  if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set output format: "
                      << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }
  // The driver may negotiate a larger sizeimage (e.g. for alignment);
  // remember it so later USERPTR/DMABUF submissions advertise correctly.
  frame_size_ = std::max<int>(frame_size_, fmt.fmt.pix_mp.plane_fmt[0].sizeimage);

  // --- Set CAPTURE format (H.264 bitstream from the encoder) ---

  fmt = {};
  fmt.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  fmt.fmt.pix_mp.width = width;
  fmt.fmt.pix_mp.height = height;
  fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_H264;
  fmt.fmt.pix_mp.field = V4L2_FIELD_ANY;
  fmt.fmt.pix_mp.colorspace = V4L2_COLORSPACE_DEFAULT;
  fmt.fmt.pix_mp.num_planes = 1;
  fmt.fmt.pix_mp.plane_fmt[0].bytesperline = 0;
  fmt.fmt.pix_mp.plane_fmt[0].sizeimage = 512 << 10;  // 512 KiB
  if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set capture format: "
                      << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }

  // --- Set framerate via stream parameters ---

  if (framerate > 0) {
    struct v4l2_streamparm parm = {};
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    parm.parm.output.timeperframe.numerator = 1;
    parm.parm.output.timeperframe.denominator = framerate;
    if (Xioctl(fd_, VIDIOC_S_PARM, &parm) < 0)
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set framerate: "
                          << strerror(errno);
  }

  // --- Configure encoder controls (after format negotiation) ---

  // Bitrate must be constant for WebRTC's congestion control to behave
  // predictably; relying on the driver default is fragile.
  TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_BITRATE_MODE,
                V4L2_MPEG_VIDEO_BITRATE_MODE_CBR, "bitrate mode (CBR)");

  if (bitrate > 0) {
    TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_BITRATE, bitrate, "bitrate");
  }

  // H.264 profile -- prefer Constrained Baseline for maximum WebRTC
  // compatibility; fall back to plain Baseline if the driver doesn't
  // support the constrained variant.
  v4l2_control ctrl = {};
  ctrl.id = V4L2_CID_MPEG_VIDEO_H264_PROFILE;
  ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE;
  if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0)
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set H.264 profile: "
                          << strerror(errno);
  }

  // H.264 level 3.1 -- matches the SDP profile-level-id ("42e01f")
  // advertised by V4L2VideoEncoderFactory. Supports up to 1280x720@30.
  TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_H264_LEVEL,
                V4L2_MPEG_VIDEO_H264_LEVEL_3_1, "H.264 level");

  // Keyframe (IDR) interval in frames.
  if (keyframe_interval > 0) {
    TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_H264_I_PERIOD, keyframe_interval,
                  "intra period");
  }

  // Repeat SPS/PPS headers before every IDR -- required for WebRTC so
  // that late-joining subscribers can decode immediately.
  TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_REPEAT_SEQ_HEADER, 1,
                "inline headers");

  // --- Request OUTPUT buffers (mmap'd only in Mmap mode) ---

  v4l2_requestbuffers reqbufs = {};
  reqbufs.count = kNumOutputBuffers;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  reqbufs.memory = V4l2MemoryFor(mode_);
  if (Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to request output buffers (mode "
                      << ModeName(mode_) << "): " << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }
  // Clamp to the fixed-size array. A well-behaved driver returns at most
  // |count|, but we defensively cap to avoid any chance of overflow.
  num_output_buffers_ = std::min<int>(reqbufs.count, kNumOutputBuffers);
  if (num_output_buffers_ < kMinBuffersPerQueue) {
    RTC_LOG(LS_ERROR) << "V4L2: Driver returned only " << num_output_buffers_
                      << " output buffers (minimum " << kMinBuffersPerQueue
                      << ")";
    close(fd_);
    fd_ = -1;
    return false;
  }
  RTC_LOG(LS_INFO) << "V4L2: Allocated " << num_output_buffers_
                   << " output buffers";

  if (mode_ == OutputBufferMode::Mmap) {
    for (int i = 0; i < num_output_buffers_; i++) {
      v4l2_plane planes[VIDEO_MAX_PLANES] = {};
      v4l2_buffer buf = {};
      buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
      buf.memory = V4L2_MEMORY_MMAP;
      buf.index = i;
      buf.length = 1;
      buf.m.planes = planes;
      if (Xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
        RTC_LOG(LS_ERROR) << "V4L2: QUERYBUF output[" << i
                          << "] failed: " << strerror(errno);
        close(fd_);
        fd_ = -1;
        return false;
      }

      output_buffers_[i].length = buf.m.planes[0].length;
      output_buffers_[i].start =
          mmap(nullptr, buf.m.planes[0].length, PROT_READ | PROT_WRITE,
               MAP_SHARED, fd_, buf.m.planes[0].m.mem_offset);
      if (output_buffers_[i].start == MAP_FAILED) {
        RTC_LOG(LS_ERROR) << "V4L2: mmap output[" << i
                          << "] failed: " << strerror(errno);
        close(fd_);
        fd_ = -1;
        return false;
      }

      // Zero-fill so that any buffer the encoder references before the
      // pipeline is fully primed contains valid black YUV rather than
      // random memory (which causes green/distorted frames on Pi 4).
      memset(output_buffers_[i].start, 0, output_buffers_[i].length);
    }
  }

  // --- Request and mmap CAPTURE buffers ---

  reqbufs = {};
  reqbufs.count = kNumCaptureBuffers;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  reqbufs.memory = V4L2_MEMORY_MMAP;
  if (Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to request capture buffers: "
                      << strerror(errno);
    if (mode_ == OutputBufferMode::Mmap) {
      for (int i = 0; i < num_output_buffers_; i++) {
        if (output_buffers_[i].start && output_buffers_[i].start != MAP_FAILED)
          munmap(output_buffers_[i].start, output_buffers_[i].length);
      }
    }
    close(fd_);
    fd_ = -1;
    return false;
  }
  num_capture_buffers_ = std::min<int>(reqbufs.count, kNumCaptureBuffers);
  if (num_capture_buffers_ < kMinBuffersPerQueue) {
    RTC_LOG(LS_ERROR) << "V4L2: Driver returned only " << num_capture_buffers_
                      << " capture buffers (minimum " << kMinBuffersPerQueue
                      << ")";
    Destroy();
    return false;
  }
  RTC_LOG(LS_INFO) << "V4L2: Allocated " << num_capture_buffers_
                   << " capture buffers";

  for (int i = 0; i < num_capture_buffers_; i++) {
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    v4l2_buffer buf = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2: QUERYBUF capture[" << i
                        << "] failed: " << strerror(errno);
      Destroy();
      return false;
    }

    capture_buffers_[i].length = buf.m.planes[0].length;
    capture_buffers_[i].start =
        mmap(nullptr, buf.m.planes[0].length, PROT_READ | PROT_WRITE,
             MAP_SHARED, fd_, buf.m.planes[0].m.mem_offset);
    if (capture_buffers_[i].start == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "V4L2: mmap capture[" << i
                        << "] failed: " << strerror(errno);
      Destroy();
      return false;
    }

    // Pre-queue all capture buffers so the encoder has somewhere to write.
    buf = {};
    memset(planes, 0, sizeof(planes));
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;
    buf.m.planes[0].length = capture_buffers_[i].length;
    if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to queue capture buffer " << i
                        << ": " << strerror(errno);
      Destroy();
      return false;
    }
  }

  // --- Start streaming on both queues ---

  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  if (Xioctl(fd_, VIDIOC_STREAMON, &type) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: STREAMON output failed: " << strerror(errno);
    Destroy();
    return false;
  }

  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (Xioctl(fd_, VIDIOC_STREAMON, &type) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: STREAMON capture failed: " << strerror(errno);
    Destroy();
    return false;
  }

  initialized_ = true;
  next_output_index_ = 0;
  first_frame_ = true;

  RTC_LOG(LS_INFO) << "V4L2: H.264 encoder initialized -- " << width << "x"
                   << height << " @ " << framerate << " fps, "
                   << bitrate << " bps, mode " << ModeName(mode_);

  // Prime the pipeline in MMAP mode only; USERPTR/DMABUF priming would
  // require fabricating a contiguous user buffer or DMABUF, which the
  // caller would need to provide. The first user-submitted frame is
  // marked as IDR so the decoder still gets a clean start.
  if (mode_ == OutputBufferMode::Mmap) {
    PrimeEncoderPipeline();
  }

  return true;
}

// ---------------------------------------------------------------------------
// I420 frame copy
// ---------------------------------------------------------------------------

void V4l2H264EncoderWrapper::CopyI420ToOutputBuffer(int index,
                                                   const uint8_t* y,
                                                   const uint8_t* u,
                                                   const uint8_t* v,
                                                   int stride_y,
                                                   int stride_u,
                                                   int stride_v) {
  // The mmap'd buffer is laid out as a contiguous I420 frame:
  //   [Y plane: width * height] [U plane: w/2 * h/2] [V plane: w/2 * h/2]
  // Source strides may differ from width, so we copy row-by-row.

  uint8_t* dst = static_cast<uint8_t*>(output_buffers_[index].start);
  const int dst_stride_y = width_;
  const int dst_stride_uv = width_ / 2;
  const int chroma_height = height_ / 2;

  // Y plane -- single memcpy when src/dst strides match.
  if (stride_y == dst_stride_y) {
    memcpy(dst, y, dst_stride_y * height_);
  } else {
    for (int row = 0; row < height_; row++)
      memcpy(dst + row * dst_stride_y, y + row * stride_y, width_);
  }

  uint8_t* dst_u = dst + dst_stride_y * height_;
  if (stride_u == dst_stride_uv) {
    memcpy(dst_u, u, dst_stride_uv * chroma_height);
  } else {
    for (int row = 0; row < chroma_height; row++)
      memcpy(dst_u + row * dst_stride_uv, u + row * stride_u, dst_stride_uv);
  }

  uint8_t* dst_v = dst_u + dst_stride_uv * chroma_height;
  if (stride_v == dst_stride_uv) {
    memcpy(dst_v, v, dst_stride_uv * chroma_height);
  } else {
    for (int row = 0; row < chroma_height; row++)
      memcpy(dst_v + row * dst_stride_uv, v + row * stride_v, dst_stride_uv);
  }
}

// ---------------------------------------------------------------------------
// Pipeline priming
// ---------------------------------------------------------------------------

void V4l2H264EncoderWrapper::PrimeEncoderPipeline() {
  // The bcm2835 V4L2 M2M encoder on Raspberry Pi has internal pipeline
  // latency: the first few encoded frames may be distorted or incomplete.
  // We work around this by feeding black I420 frames through the encoder
  // and discarding the output, so the pipeline is fully warmed up before
  // any real frames arrive.

  std::vector<uint8_t> black_frame(frame_size_);

  // Build a proper black I420 frame: Y=0 (black luma), U=V=128 (neutral
  // chroma, i.e. no colour cast).
  const int y_size = width_ * height_;
  const int uv_size = y_size / 4;
  memset(black_frame.data(), 0, y_size);
  memset(black_frame.data() + y_size, 128, uv_size);
  memset(black_frame.data() + y_size + uv_size, 128, uv_size);

  const int prime_count = std::min(num_output_buffers_, 4);
  RTC_LOG(LS_INFO) << "V4L2: Priming encoder with " << prime_count
                   << " black frames";

  // --- Submit all priming frames ---

  for (int i = 0; i < prime_count; i++) {
    int idx = next_output_index_;
    next_output_index_ = (next_output_index_ + 1) % num_output_buffers_;

    memcpy(output_buffers_[idx].start, black_frame.data(), frame_size_);

    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = idx;
    buf.length = 1;
    buf.m.planes = planes;
    buf.m.planes[0].bytesused = frame_size_;
    buf.m.planes[0].length = output_buffers_[idx].length;
    if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Prime: QBUF output[" << idx
                          << "] failed: " << strerror(errno);
      break;
    }
  }

  // --- Drain all priming frames (dequeue output + capture, discard data) ---

  for (int i = 0; i < prime_count; i++) {
    pollfd pfd = {fd_, POLLIN, 0};
    if (poll(&pfd, 1, /*timeout_ms=*/500) <= 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Prime: poll timeout on frame " << i;
      break;
    }

    // Dequeue the consumed OUTPUT buffer.
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0 && errno != EAGAIN)
      RTC_LOG(LS_WARNING) << "V4L2: Prime: DQBUF output failed: "
                          << strerror(errno);

    // Dequeue the CAPTURE buffer (encoded data is discarded).
    buf = {};
    memset(planes, 0, sizeof(planes));
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
      if (errno != EAGAIN)
        RTC_LOG(LS_WARNING) << "V4L2: Prime: DQBUF capture failed: "
                            << strerror(errno);
      continue;
    }

    // Re-queue the capture buffer for future use.
    v4l2_buffer requeue = {};
    v4l2_plane rq_planes[VIDEO_MAX_PLANES] = {};
    requeue.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    requeue.memory = V4L2_MEMORY_MMAP;
    requeue.index = buf.index;
    requeue.length = 1;
    requeue.m.planes = rq_planes;
    requeue.m.planes[0].length = capture_buffers_[buf.index].length;
    Xioctl(fd_, VIDIOC_QBUF, &requeue);
  }

  // Reset so the first real Encode() call starts from buffer 0.
  next_output_index_ = 0;
  RTC_LOG(LS_INFO) << "V4L2: Encoder pipeline primed";
}

// ---------------------------------------------------------------------------
// Encoding -- planar Y/U/V input (Mmap or UserPtr)
// ---------------------------------------------------------------------------

webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface>
V4l2H264EncoderWrapper::Encode(const uint8_t* y,
                                const uint8_t* u,
                                const uint8_t* v,
                                int stride_y,
                                int stride_u,
                                int stride_v,
                                bool force_idr) {
  if (!initialized_) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode called on uninitialized encoder";
    return nullptr;
  }
  if (mode_ == OutputBufferMode::Dmabuf) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode() called on DMABUF-mode encoder; "
                         "use EncodeDmabuf instead";
    return nullptr;
  }

  // Always force an IDR on the very first frame so the decoder starts
  // with a clean reference and doesn't show startup artefacts.
  if (first_frame_) {
    force_idr = true;
    first_frame_ = false;
  }

  // Pick the next OUTPUT buffer slot (round-robin).
  const int buf_index = next_output_index_;
  next_output_index_ = (next_output_index_ + 1) % num_output_buffers_;

  const uint8_t* userptr = nullptr;

  if (mode_ == OutputBufferMode::UserPtr) {
    // USERPTR works only when the input planes are arranged as a single
    // contiguous I420 buffer matching what the encoder expects:
    //   [Y: stride_y == width, height rows] [U: stride_u == width/2, chroma_height rows] [V: ...]
    const int dst_stride_y = width_;
    const int dst_stride_uv = width_ / 2;
    const int chroma_height = height_ / 2;
    const bool strides_match = (stride_y == dst_stride_y) &&
                               (stride_u == dst_stride_uv) &&
                               (stride_v == dst_stride_uv);
    const bool planes_contiguous =
        strides_match &&
        (u == y + static_cast<ptrdiff_t>(dst_stride_y) * height_) &&
        (v == u + static_cast<ptrdiff_t>(dst_stride_uv) * chroma_height);
    if (planes_contiguous) {
      userptr = y;
    } else {
      RTC_LOG_F(LS_WARNING)
          << "V4L2: USERPTR fast path declined (non-contiguous planes); "
             "falling back to a temp copy";
      // We requested USERPTR buffers, so we can't memcpy into a driver
      // buffer. Allocate a small heap buffer and use it as the userptr.
      // This is rare and not in the hot path -- it only happens when
      // upstream WebRTC scaled or cropped the frame.
      static thread_local std::vector<uint8_t> scratch;
      scratch.resize(frame_size_);
      uint8_t* dst = scratch.data();
      if (stride_y == dst_stride_y) {
        memcpy(dst, y, dst_stride_y * height_);
      } else {
        for (int row = 0; row < height_; row++)
          memcpy(dst + row * dst_stride_y, y + row * stride_y, width_);
      }
      uint8_t* dst_u = dst + dst_stride_y * height_;
      if (stride_u == dst_stride_uv) {
        memcpy(dst_u, u, dst_stride_uv * chroma_height);
      } else {
        for (int row = 0; row < chroma_height; row++)
          memcpy(dst_u + row * dst_stride_uv, u + row * stride_u,
                 dst_stride_uv);
      }
      uint8_t* dst_v = dst_u + dst_stride_uv * chroma_height;
      if (stride_v == dst_stride_uv) {
        memcpy(dst_v, v, dst_stride_uv * chroma_height);
      } else {
        for (int row = 0; row < chroma_height; row++)
          memcpy(dst_v + row * dst_stride_uv, v + row * stride_v,
                 dst_stride_uv);
      }
      userptr = dst;
    }
  } else {
    // MMAP: copy the caller's I420 frame into the mmap'd buffer.
    CopyI420ToOutputBuffer(buf_index, y, u, v, stride_y, stride_u, stride_v);
  }

  return RunEncode(buf_index, force_idr, userptr, /*dmabuf_fd=*/-1,
                   /*offset=*/0, /*length=*/0);
}

// ---------------------------------------------------------------------------
// Encoding -- DMABUF input
// ---------------------------------------------------------------------------

webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface>
V4l2H264EncoderWrapper::EncodeDmabuf(int dmabuf_fd,
                                      size_t offset,
                                      size_t length,
                                      bool force_idr) {
  if (!initialized_) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called on uninitialized encoder";
    return nullptr;
  }
  if (mode_ != OutputBufferMode::Dmabuf) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called but encoder is in "
                      << ModeName(mode_) << " mode";
    return nullptr;
  }
  if (dmabuf_fd < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called with invalid fd";
    return nullptr;
  }

  if (first_frame_) {
    force_idr = true;
    first_frame_ = false;
  }

  const int buf_index = next_output_index_;
  next_output_index_ = (next_output_index_ + 1) % num_output_buffers_;

  return RunEncode(buf_index, force_idr, /*userptr=*/nullptr, dmabuf_fd, offset,
                   length == 0 ? static_cast<size_t>(frame_size_) : length);
}

// ---------------------------------------------------------------------------
// Shared encode-submit/dequeue path
// ---------------------------------------------------------------------------

webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface>
V4l2H264EncoderWrapper::RunEncode(int buf_index,
                                   bool force_idr,
                                   const uint8_t* userptr,
                                   int dmabuf_fd,
                                   size_t offset,
                                   size_t length) {
  if (force_idr) {
    v4l2_control ctrl = {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
    ctrl.value = 1;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0)
      RTC_LOG(LS_WARNING) << "V4L2: Failed to force IDR: " << strerror(errno);
  }

  // Queue the OUTPUT buffer for encoding.
  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4l2MemoryFor(mode_);
  buf.index = buf_index;
  buf.length = 1;
  buf.m.planes = planes;
  buf.m.planes[0].bytesused = static_cast<uint32_t>(
      mode_ == OutputBufferMode::Dmabuf ? length : frame_size_);

  switch (mode_) {
    case OutputBufferMode::Mmap:
      buf.m.planes[0].length = output_buffers_[buf_index].length;
      break;
    case OutputBufferMode::UserPtr:
      buf.m.planes[0].length = frame_size_;
      buf.m.planes[0].m.userptr = reinterpret_cast<unsigned long>(userptr);
      break;
    case OutputBufferMode::Dmabuf:
      buf.m.planes[0].length = static_cast<uint32_t>(length);
      buf.m.planes[0].data_offset = static_cast<uint32_t>(offset);
      buf.m.planes[0].m.fd = dmabuf_fd;
      break;
  }

  if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: QBUF output failed (mode " << ModeName(mode_)
                      << "): " << strerror(errno);
    return nullptr;
  }

  // Wait for the encoder to produce data (1 s timeout).
  pollfd pfd = {fd_, POLLIN, 0};
  if (poll(&pfd, 1, /*timeout_ms=*/1000) <= 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Poll timeout waiting for encoded data: "
                      << strerror(errno);
    return nullptr;
  }

  // Dequeue the consumed OUTPUT buffer (non-fatal if EAGAIN).
  buf = {};
  memset(planes, 0, sizeof(planes));
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4l2MemoryFor(mode_);
  buf.length = 1;
  buf.m.planes = planes;
  if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0 && errno != EAGAIN)
    RTC_LOG(LS_ERROR) << "V4L2: DQBUF output failed: " << strerror(errno);

  // Dequeue the CAPTURE buffer containing the encoded H.264 bitstream.
  buf = {};
  memset(planes, 0, sizeof(planes));
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 1;
  buf.m.planes = planes;
  if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: DQBUF capture failed: " << strerror(errno);
    return nullptr;
  }

  // Construct the EncodedImageBuffer directly from the mmap'd CAPTURE
  // data (a single allocation + copy) before requeueing. This eliminates
  // the std::vector round-trip the previous implementation paid.
  const size_t encoded_size = buf.m.planes[0].bytesused;
  webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface> out;
  if (encoded_size > 0) {
    auto* data = static_cast<const uint8_t*>(capture_buffers_[buf.index].start);
    out = webrtc::EncodedImageBuffer::Create(data, encoded_size);
  }

  // Re-queue the CAPTURE buffer so the encoder can reuse it.
  v4l2_buffer requeue = {};
  v4l2_plane rq_planes[VIDEO_MAX_PLANES] = {};
  requeue.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  requeue.memory = V4L2_MEMORY_MMAP;
  requeue.index = buf.index;
  requeue.length = 1;
  requeue.m.planes = rq_planes;
  requeue.m.planes[0].length = capture_buffers_[buf.index].length;
  if (Xioctl(fd_, VIDIOC_QBUF, &requeue) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to re-queue capture buffer: "
                      << strerror(errno);
    return nullptr;
  }

  return out;
}

// ---------------------------------------------------------------------------
// Runtime rate updates
// ---------------------------------------------------------------------------

void V4l2H264EncoderWrapper::UpdateRates(int framerate, int bitrate) {
  if (fd_ < 0)
    return;

  if (bitrate > 0) {
    TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_BITRATE, bitrate, "bitrate");
  }

  if (framerate > 0 && framerate != framerate_) {
    framerate_ = framerate;
    struct v4l2_streamparm parm = {};
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    parm.parm.output.timeperframe.numerator = 1;
    parm.parm.output.timeperframe.denominator = framerate;
    if (Xioctl(fd_, VIDIOC_S_PARM, &parm) < 0)
      RTC_LOG(LS_WARNING) << "V4L2: Failed to update framerate: "
                          << strerror(errno);
  }
}

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------

void V4l2H264EncoderWrapper::Destroy() {
  if (fd_ < 0) {
    initialized_ = false;
    return;
  }

  // 1. Stop both streaming queues.
  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);

  // 2. Unmap and release OUTPUT buffers (only mapped in Mmap mode).
  if (mode_ == OutputBufferMode::Mmap) {
    for (int i = 0; i < num_output_buffers_; i++) {
      if (output_buffers_[i].start && output_buffers_[i].start != MAP_FAILED) {
        munmap(output_buffers_[i].start, output_buffers_[i].length);
        output_buffers_[i].start = nullptr;
      }
    }
  }
  v4l2_requestbuffers reqbufs = {};
  reqbufs.count = 0;  // free all buffers
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  reqbufs.memory = V4l2MemoryFor(mode_);
  Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs);

  // 3. Unmap and release CAPTURE buffers.
  for (int i = 0; i < num_capture_buffers_; i++) {
    if (capture_buffers_[i].start && capture_buffers_[i].start != MAP_FAILED) {
      munmap(capture_buffers_[i].start, capture_buffers_[i].length);
      capture_buffers_[i].start = nullptr;
    }
  }
  reqbufs = {};
  reqbufs.count = 0;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  reqbufs.memory = V4L2_MEMORY_MMAP;
  Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs);

  // 4. Close the device.
  close(fd_);
  fd_ = -1;
  num_output_buffers_ = 0;
  num_capture_buffers_ = 0;
  initialized_ = false;

  RTC_LOG(LS_INFO) << "V4L2: Encoder destroyed";
}

}  // namespace livekit_ffi
