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

#include "rtc_base/logging.h"

namespace livekit_ffi {

int V4l2H264EncoderWrapper::Xioctl(int fd, unsigned long ctl, void* arg) {
  int ret;
  int tries = 10;
  do {
    ret = ioctl(fd, ctl, arg);
  } while (ret == -1 && errno == EINTR && tries-- > 0);
  return ret;
}

V4l2H264EncoderWrapper::V4l2H264EncoderWrapper() {}

V4l2H264EncoderWrapper::~V4l2H264EncoderWrapper() {
  if (initialized_) {
    Destroy();
  }
}

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
    if (name.find("video") != 0) {
      continue;
    }

    std::string path = "/dev/" + name;
    int fd = open(path.c_str(), O_RDWR | O_NONBLOCK, 0);
    if (fd < 0) {
      continue;
    }

    // Query device capabilities.
    struct v4l2_capability cap = {};
    if (Xioctl(fd, VIDIOC_QUERYCAP, &cap) < 0) {
      close(fd);
      continue;
    }

    // We need a M2M device with multiplanar support.
    bool is_m2m = (cap.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE) != 0;
    if (!is_m2m) {
      // Some drivers report the capability on the device_caps instead.
      is_m2m = (cap.device_caps & V4L2_CAP_VIDEO_M2M_MPLANE) != 0;
    }
    if (!is_m2m) {
      close(fd);
      continue;
    }

    // Check if the capture side supports H.264 output.
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

bool V4l2H264EncoderWrapper::Initialize(int width,
                                         int height,
                                         int bitrate,
                                         int keyframe_interval,
                                         int framerate,
                                         const std::string& device_path) {
  if (initialized_) {
    Destroy();
  }

  width_ = width;
  height_ = height;
  framerate_ = framerate;

  std::string path = device_path;
  if (path.empty()) {
    path = FindEncoderDevice();
  }
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
  RTC_LOG(LS_INFO) << "V4L2: Opened encoder device " << path << " as fd " << fd_;

  // --- Set encoder controls ---

  v4l2_control ctrl = {};

  // Bitrate
  if (bitrate > 0) {
    ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE;
    ctrl.value = bitrate;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set bitrate: " << strerror(errno);
    }
  }

  // Profile: constrained baseline for maximum WebRTC compatibility.
  ctrl = {};
  ctrl.id = V4L2_CID_MPEG_VIDEO_H264_PROFILE;
  ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE;
  if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    // Fall back to baseline if constrained baseline is not supported.
    ctrl.value = V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set H.264 profile: " << strerror(errno);
    }
  }

  // Level 4.0
  ctrl = {};
  ctrl.id = V4L2_CID_MPEG_VIDEO_H264_LEVEL;
  ctrl.value = V4L2_MPEG_VIDEO_H264_LEVEL_4_0;
  if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "V4L2: Failed to set H.264 level: " << strerror(errno);
  }

  // Intra period (keyframe interval)
  if (keyframe_interval > 0) {
    ctrl = {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_H264_I_PERIOD;
    ctrl.value = keyframe_interval;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set intra period: " << strerror(errno);
    }
  }

  // Inline SPS/PPS headers with every IDR (required for WebRTC).
  ctrl = {};
  ctrl.id = V4L2_CID_MPEG_VIDEO_REPEAT_SEQ_HEADER;
  ctrl.value = 1;
  if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "V4L2: Failed to set inline headers: " << strerror(errno);
  }

  // --- Set output format (encoder input): YUV420 ---

  v4l2_format fmt = {};
  fmt.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  fmt.fmt.pix_mp.width = width;
  fmt.fmt.pix_mp.height = height;
  fmt.fmt.pix_mp.pixelformat = V4L2_PIX_FMT_YUV420;
  fmt.fmt.pix_mp.field = V4L2_FIELD_ANY;
  fmt.fmt.pix_mp.colorspace = V4L2_COLORSPACE_SMPTE170M;
  fmt.fmt.pix_mp.num_planes = 1;
  fmt.fmt.pix_mp.plane_fmt[0].bytesperline = width;
  fmt.fmt.pix_mp.plane_fmt[0].sizeimage = width * height * 3 / 2;
  if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set output format: " << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }

  // --- Set capture format (encoder output): H.264 ---

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
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set capture format: " << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }

  // --- Set framerate ---

  if (framerate > 0) {
    struct v4l2_streamparm parm = {};
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    parm.parm.output.timeperframe.numerator = 1;
    parm.parm.output.timeperframe.denominator = framerate;
    if (Xioctl(fd_, VIDIOC_S_PARM, &parm) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to set framerate: " << strerror(errno);
    }
  }

  // --- Request and mmap output buffers (encoder input) ---

  v4l2_requestbuffers reqbufs = {};
  reqbufs.count = NUM_OUTPUT_BUFFERS;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  reqbufs.memory = V4L2_MEMORY_MMAP;
  if (Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to request output buffers: " << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }
  num_output_buffers_ = reqbufs.count;
  RTC_LOG(LS_INFO) << "V4L2: Got " << num_output_buffers_ << " output buffers";

  for (int i = 0; i < num_output_buffers_; i++) {
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    v4l2_buffer buf = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to query output buffer " << i
                        << ": " << strerror(errno);
      close(fd_);
      fd_ = -1;
      return false;
    }
    output_buffers_[i].length = buf.m.planes[0].length;
    output_buffers_[i].start = mmap(nullptr, buf.m.planes[0].length,
                                     PROT_READ | PROT_WRITE, MAP_SHARED,
                                     fd_, buf.m.planes[0].m.mem_offset);
    if (output_buffers_[i].start == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to mmap output buffer " << i
                        << ": " << strerror(errno);
      close(fd_);
      fd_ = -1;
      return false;
    }
  }

  // --- Request and mmap capture buffers (encoder output) ---

  reqbufs = {};
  reqbufs.count = NUM_CAPTURE_BUFFERS;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  reqbufs.memory = V4L2_MEMORY_MMAP;
  if (Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to request capture buffers: " << strerror(errno);
    // Cleanup output buffers.
    for (int i = 0; i < num_output_buffers_; i++) {
      if (output_buffers_[i].start && output_buffers_[i].start != MAP_FAILED) {
        munmap(output_buffers_[i].start, output_buffers_[i].length);
      }
    }
    close(fd_);
    fd_ = -1;
    return false;
  }
  num_capture_buffers_ = reqbufs.count;
  RTC_LOG(LS_INFO) << "V4L2: Got " << num_capture_buffers_ << " capture buffers";

  for (int i = 0; i < num_capture_buffers_; i++) {
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    v4l2_buffer buf = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.index = i;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_QUERYBUF, &buf) < 0) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to query capture buffer " << i
                        << ": " << strerror(errno);
      Destroy();
      return false;
    }
    capture_buffers_[i].length = buf.m.planes[0].length;
    capture_buffers_[i].start = mmap(nullptr, buf.m.planes[0].length,
                                      PROT_READ | PROT_WRITE, MAP_SHARED,
                                      fd_, buf.m.planes[0].m.mem_offset);
    if (capture_buffers_[i].start == MAP_FAILED) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to mmap capture buffer " << i
                        << ": " << strerror(errno);
      Destroy();
      return false;
    }

    // Queue all capture buffers so the encoder can write into them.
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
    RTC_LOG(LS_ERROR) << "V4L2: Failed to start output streaming: " << strerror(errno);
    Destroy();
    return false;
  }

  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (Xioctl(fd_, VIDIOC_STREAMON, &type) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to start capture streaming: " << strerror(errno);
    Destroy();
    return false;
  }

  RTC_LOG(LS_INFO) << "V4L2: H.264 encoder initialized at " << width << "x" << height
                   << " @ " << framerate << " fps, bitrate " << bitrate;
  initialized_ = true;
  next_output_index_ = 0;
  return true;
}

void V4l2H264EncoderWrapper::CopyI420ToOutputBuffer(int index,
                                                      const uint8_t* y,
                                                      const uint8_t* u,
                                                      const uint8_t* v,
                                                      int stride_y,
                                                      int stride_u,
                                                      int stride_v) {
  uint8_t* dst = static_cast<uint8_t*>(output_buffers_[index].start);
  int dst_stride_y = width_;
  int dst_stride_uv = width_ / 2;

  // Copy Y plane
  for (int row = 0; row < height_; row++) {
    memcpy(dst + row * dst_stride_y, y + row * stride_y, width_);
  }

  // Copy U plane
  uint8_t* dst_u = dst + dst_stride_y * height_;
  for (int row = 0; row < height_ / 2; row++) {
    memcpy(dst_u + row * dst_stride_uv, u + row * stride_u, width_ / 2);
  }

  // Copy V plane
  uint8_t* dst_v = dst_u + dst_stride_uv * (height_ / 2);
  for (int row = 0; row < height_ / 2; row++) {
    memcpy(dst_v + row * dst_stride_uv, v + row * stride_v, width_ / 2);
  }
}

bool V4l2H264EncoderWrapper::Encode(const uint8_t* y,
                                     const uint8_t* u,
                                     const uint8_t* v,
                                     int stride_y,
                                     int stride_u,
                                     int stride_v,
                                     bool forceIDR,
                                     std::vector<uint8_t>& output) {
  if (!initialized_) {
    RTC_LOG(LS_ERROR) << "V4L2: Encoder not initialized";
    return false;
  }

  // Request a keyframe if needed.
  if (forceIDR) {
    v4l2_control ctrl = {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
    ctrl.value = 1;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to force keyframe: " << strerror(errno);
    }
  }

  int buf_index = next_output_index_;
  next_output_index_ = (next_output_index_ + 1) % num_output_buffers_;

  // Copy the I420 frame into the mmap'd output buffer.
  CopyI420ToOutputBuffer(buf_index, y, u, v, stride_y, stride_u, stride_v);

  // Queue the output buffer (encoder input).
  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.index = buf_index;
  buf.length = 1;
  buf.m.planes = planes;
  buf.m.planes[0].bytesused = width_ * height_ * 3 / 2;
  buf.m.planes[0].length = output_buffers_[buf_index].length;
  if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to queue output buffer: " << strerror(errno);
    return false;
  }

  // Poll for the encoder to produce output.
  pollfd pfd = {fd_, POLLIN, 0};
  int ret = poll(&pfd, 1, 1000);  // 1 second timeout
  if (ret <= 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Poll timeout or error waiting for encoder: "
                      << strerror(errno);
    return false;
  }

  // Dequeue the output buffer (encoder input done).
  buf = {};
  memset(planes, 0, sizeof(planes));
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 1;
  buf.m.planes = planes;
  if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
    if (errno != EAGAIN) {
      RTC_LOG(LS_ERROR) << "V4L2: Failed to dequeue output buffer: " << strerror(errno);
    }
    // Non-fatal: the output buffer may not be ready yet.
  }

  // Dequeue the capture buffer (encoded H.264 data).
  buf = {};
  memset(planes, 0, sizeof(planes));
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.length = 1;
  buf.m.planes = planes;
  if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to dequeue capture buffer: " << strerror(errno);
    return false;
  }

  // Copy encoded data to output vector.
  size_t encoded_size = buf.m.planes[0].bytesused;
  if (encoded_size > 0) {
    uint8_t* encoded_data = static_cast<uint8_t*>(capture_buffers_[buf.index].start);
    output.assign(encoded_data, encoded_data + encoded_size);
  }

  // Re-queue the capture buffer for reuse.
  v4l2_buffer requeue_buf = {};
  v4l2_plane requeue_planes[VIDEO_MAX_PLANES] = {};
  requeue_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  requeue_buf.memory = V4L2_MEMORY_MMAP;
  requeue_buf.index = buf.index;
  requeue_buf.length = 1;
  requeue_buf.m.planes = requeue_planes;
  requeue_buf.m.planes[0].length = capture_buffers_[buf.index].length;
  if (Xioctl(fd_, VIDIOC_QBUF, &requeue_buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to re-queue capture buffer: " << strerror(errno);
    return false;
  }

  return encoded_size > 0;
}

void V4l2H264EncoderWrapper::UpdateRates(int framerate, int bitrate) {
  if (fd_ < 0) {
    return;
  }

  if (bitrate > 0) {
    v4l2_control ctrl = {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_BITRATE;
    ctrl.value = bitrate;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to update bitrate: " << strerror(errno);
    }
  }

  if (framerate > 0 && framerate != framerate_) {
    framerate_ = framerate;
    struct v4l2_streamparm parm = {};
    parm.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    parm.parm.output.timeperframe.numerator = 1;
    parm.parm.output.timeperframe.denominator = framerate;
    if (Xioctl(fd_, VIDIOC_S_PARM, &parm) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Failed to update framerate: " << strerror(errno);
    }
  }
}

void V4l2H264EncoderWrapper::Destroy() {
  if (fd_ < 0) {
    initialized_ = false;
    return;
  }

  // Stop streaming.
  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);

  // Unmap and free output buffers.
  for (int i = 0; i < num_output_buffers_; i++) {
    if (output_buffers_[i].start && output_buffers_[i].start != MAP_FAILED) {
      munmap(output_buffers_[i].start, output_buffers_[i].length);
      output_buffers_[i].start = nullptr;
    }
  }
  v4l2_requestbuffers reqbufs = {};
  reqbufs.count = 0;
  reqbufs.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  reqbufs.memory = V4L2_MEMORY_MMAP;
  Xioctl(fd_, VIDIOC_REQBUFS, &reqbufs);

  // Unmap and free capture buffers.
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

  close(fd_);
  fd_ = -1;
  num_output_buffers_ = 0;
  num_capture_buffers_ = 0;
  initialized_ = false;

  RTC_LOG(LS_INFO) << "V4L2: Encoder destroyed";
}

}  // namespace livekit_ffi
