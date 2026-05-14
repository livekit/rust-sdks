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
#include <stdint.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <sys/time.h>
#include <unistd.h>

#include <linux/videodev2.h>

#include <algorithm>
#include <cerrno>
#include <chrono>
#include <cstdio>
#include <cstdint>
#include <string>
#include <utility>
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

static int ChromaHeight(int height) {
  return (height + 1) / 2;
}

static int ChromaWidth(int width) {
  return (width + 1) / 2;
}

static int AlignUp(int value, int alignment) {
  return ((value + alignment - 1) / alignment) * alignment;
}

static int ChromaStrideForFourcc(uint32_t fourcc, int stride) {
  if (fourcc == V4L2_PIX_FMT_YUV420) {
    return stride / 2;
  }
  if (fourcc == V4L2_PIX_FMT_NV12) {
    return stride;
  }
  return stride;
}

// Compute the byte size of a contiguous planar frame for the given fourcc.
static int FrameSizeForFourcc(uint32_t fourcc, int stride, int height) {
  const int chroma_height = ChromaHeight(height);
  if (fourcc == V4L2_PIX_FMT_YUV420) {
    return stride * height + 2 * ChromaStrideForFourcc(fourcc, stride) * chroma_height;
  }
  if (fourcc == V4L2_PIX_FMT_NV12) {
    return stride * height + ChromaStrideForFourcc(fourcc, stride) * chroma_height;
  }
  // Conservative fallback: treat as 1 byte/pixel.
  return stride * height;
}

static int StorageLumaHeightForFourcc(uint32_t fourcc,
                                      int stride,
                                      int visible_height,
                                      int sizeimage) {
  if (fourcc != V4L2_PIX_FMT_YUV420 && fourcc != V4L2_PIX_FMT_NV12)
    return visible_height;

  // bcm2835-codec stores raw OUTPUT planes at macroblock-aligned heights
  // for H.264, even when the visible frame height is not a multiple of 16
  // (for example 480x360). The negotiated sizeimage tells us when that
  // storage layout is available; keep visible-height offsets otherwise.
  const int aligned_height = AlignUp(visible_height, 16);
  if (aligned_height == visible_height)
    return visible_height;
  if (FrameSizeForFourcc(fourcc, stride, aligned_height) <= sizeimage)
    return aligned_height;
  return visible_height;
}

static timeval TimevalFromUsec(uint64_t timestamp_us) {
  timeval tv = {};
  tv.tv_sec = static_cast<time_t>(timestamp_us / 1000000);
  tv.tv_usec = static_cast<suseconds_t>(timestamp_us % 1000000);
  return tv;
}

static uint64_t TimevalToUsec(const timeval& tv) {
  return static_cast<uint64_t>(tv.tv_sec) * 1000000 +
         static_cast<uint64_t>(tv.tv_usec);
}

static std::string FourccToString(uint32_t fourcc) {
  char fourcc_chars[5] = {
      static_cast<char>(fourcc & 0xff),
      static_cast<char>((fourcc >> 8) & 0xff),
      static_cast<char>((fourcc >> 16) & 0xff),
      static_cast<char>((fourcc >> 24) & 0xff),
      '\0',
  };
  return std::string(fourcc_chars);
}

struct H264LevelSelection {
  int32_t control_value;
  const char* level_name;
  const char* profile_level_id;
  int macroblocks_per_frame;
  int macroblocks_per_second;
  bool capped_at_level_42;
};

static H264LevelSelection SelectH264Level(int width,
                                          int height,
                                          int framerate) {
  const int mb_width = AlignUp(width, 16) / 16;
  const int mb_height = AlignUp(height, 16) / 16;
  const int mb_per_frame = mb_width * mb_height;
  const int mb_per_second = mb_per_frame * std::max(1, framerate);

  // H.264 Annex A limits. Although 720p30 lands exactly on the level 3.1
  // macroblock-rate boundary, bcm2835-codec on Pi can behave as if it is
  // still capped lower. Use level 4.0 at and above that boundary.
  if (mb_per_frame < 3600 && mb_per_second < 108000) {
    return {V4L2_MPEG_VIDEO_H264_LEVEL_3_1, "3.1", "42e01f",
            mb_per_frame, mb_per_second, false};
  }
  if (mb_per_frame <= 8192 && mb_per_second <= 245760) {
    return {V4L2_MPEG_VIDEO_H264_LEVEL_4_0, "4.0", "42e028",
            mb_per_frame, mb_per_second, false};
  }
  return {V4L2_MPEG_VIDEO_H264_LEVEL_4_2, "4.2", "42e02a",
          mb_per_frame, mb_per_second,
          mb_per_frame > 8704 || mb_per_second > 522240};
}

struct H264AccessUnitInfo {
  bool saw_nal = false;
  bool has_idr = false;
  bool has_sps = false;
  bool has_pps = false;
};

static const uint8_t* FindH264StartCode(const uint8_t* begin,
                                        const uint8_t* end,
                                        int* start_code_size) {
  for (const uint8_t* p = begin; p + 3 <= end; ++p) {
    if (p[0] != 0 || p[1] != 0)
      continue;
    if (p[2] == 1) {
      *start_code_size = 3;
      return p;
    }
    if (p + 4 <= end && p[2] == 0 && p[3] == 1) {
      *start_code_size = 4;
      return p;
    }
  }
  *start_code_size = 0;
  return end;
}

static void RecordH264NalType(uint8_t nal_header, H264AccessUnitInfo* info) {
  info->saw_nal = true;
  switch (nal_header & 0x1f) {
    case 5:
      info->has_idr = true;
      break;
    case 7:
      info->has_sps = true;
      break;
    case 8:
      info->has_pps = true;
      break;
  }
}

static H264AccessUnitInfo InspectH264AccessUnit(const uint8_t* data,
                                                size_t size) {
  H264AccessUnitInfo info;
  if (!data || size == 0)
    return info;

  const uint8_t* end = data + size;
  int start_code_size = 0;
  const uint8_t* start = FindH264StartCode(data, end, &start_code_size);
  if (start == end) {
    // Fallback for a single raw NAL unit. V4L2 H.264 output is normally
    // Annex B byte stream, but this keeps keyframe metadata conservative if
    // a driver emits one bare NAL.
    RecordH264NalType(data[0], &info);
    return info;
  }

  while (start < end) {
    const uint8_t* nal = start + start_code_size;
    int next_start_code_size = 0;
    const uint8_t* next = FindH264StartCode(nal, end, &next_start_code_size);
    if (nal < next)
      RecordH264NalType(nal[0], &info);
    start = next;
    start_code_size = next_start_code_size;
  }

  return info;
}

static EncodeResult EncodeNoOutput() {
  EncodeResult result;
  result.status = EncodeResult::Status::NoOutput;
  return result;
}

static EncodeResult EncodeError() {
  EncodeResult result;
  result.status = EncodeResult::Status::Error;
  return result;
}

static EncodeResult EncodeOk(EncodedFrame frame) {
  EncodeResult result;
  result.status = EncodeResult::Status::Ok;
  result.frame = std::move(frame);
  return result;
}

// Set a single V4L2 control, logging a warning on failure but not aborting.
static bool TrySetControl(int fd, uint32_t id, int32_t value, const char* name) {
  v4l2_control ctrl = {};
  ctrl.id = id;
  ctrl.value = value;
  if (V4l2H264EncoderWrapper::Xioctl(fd, VIDIOC_S_CTRL, &ctrl) < 0) {
    RTC_LOG(LS_WARNING) << "V4L2: Failed to set " << name << ": "
                        << strerror(errno);
    return false;
  }

  v4l2_control readback = {};
  readback.id = id;
  if (V4l2H264EncoderWrapper::Xioctl(fd, VIDIOC_G_CTRL, &readback) == 0) {
    if (readback.value != value) {
      RTC_LOG(LS_WARNING) << "V4L2: " << name << " read back as "
                          << readback.value << " after setting " << value;
    } else {
      RTC_LOG(LS_VERBOSE) << "V4L2: set " << name << " to " << value;
    }
  }
  return true;
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
      RTC_LOG(LS_VERBOSE) << "V4L2: Found H.264 M2M encoder at " << path
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
                                       int input_stride,
                                       uint32_t input_colorspace_v4l2,
                                       const std::string& device_path) {
  if (initialized_)
    Destroy();

  width_ = width;
  height_ = height;
  framerate_ = framerate > 0 ? framerate : 30;
  bitrate_ = bitrate;
  mode_ = mode;
  input_fourcc_ = input_fourcc;
  output_stride_ = input_stride > 0 ? input_stride : width;
  output_chroma_stride_ = ChromaStrideForFourcc(input_fourcc_, output_stride_);
  output_luma_height_ = height;
  output_chroma_height_ = ChromaHeight(height);
  frame_size_ = FrameSizeForFourcc(input_fourcc_, output_stride_, height);
  capture_buffer_size_ = std::max(2 << 20, width * height);
  pending_frames_.clear();
  ready_frames_.clear();
  next_v4l2_timestamp_us_ = 1;
  force_next_keyframe_ = false;
  require_next_keyframe_parameter_sets_ = true;
  for (int i = 0; i < kNumOutputBuffers; ++i)
    output_buffer_queued_[i] = false;
  for (int i = 0; i < kNumOutputBuffers; ++i)
    retained_input_buffers_[i] = nullptr;

  // --- Open the encoder device ---

  std::string path = device_path;
  if (path.empty())
    path = FindEncoderDevice();
  if (path.empty()) {
    RTC_LOG(LS_ERROR) << "V4L2: No H.264 M2M encoder device found";
    return false;
  }

  fd_ = open(path.c_str(), O_RDWR | O_NONBLOCK, 0);
  if (fd_ < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to open " << path
                      << ": " << strerror(errno);
    return false;
  }
  RTC_LOG(LS_INFO) << "V4L2: Opened encoder device " << path
                   << " (fd " << fd_ << ", mode " << ModeName(mode_) << ")";

  // --- Configure encoder controls (before format negotiation) ---
  //
  // bcm2835-codec exposes the H.264 controls based on the device node
  // alone, so they're settable before either format is configured.
  // rpicam-apps applies controls first; mirror that order here so that
  // S_FMT calls don't clobber any control-derived defaults.

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

  const H264LevelSelection h264_level =
      SelectH264Level(width, height, framerate_);
  RTC_LOG(LS_INFO) << "V4L2: selected H.264 level " << h264_level.level_name
                   << " (profile-level-id "
                   << h264_level.profile_level_id << ") for " << width << "x"
                   << height << " @ " << framerate_ << " fps, "
                   << h264_level.macroblocks_per_frame << " MB/frame, "
                   << h264_level.macroblocks_per_second << " MB/s";
  if (h264_level.capped_at_level_42) {
    RTC_LOG(LS_WARNING)
        << "V4L2: requested H.264 size/rate exceeds level 4.2 limits; "
           "driver may reject the stream or encode below the requested rate";
  }
  TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_H264_LEVEL,
                h264_level.control_value, "H.264 level");

  // Keyframe (IDR) interval in frames.
  if (keyframe_interval > 0) {
    TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_H264_I_PERIOD, keyframe_interval,
                  "intra period");
  }

  // Repeat SPS/PPS headers before every IDR -- required for WebRTC so
  // that late-joining subscribers can decode immediately.
  TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_REPEAT_SEQ_HEADER, 1,
                "inline headers");

  // --- Set OUTPUT format (raw YUV fed into the encoder) ---
  //
  // rpicam-apps sets OUTPUT first then CAPTURE; bcm2835-codec accepts
  // either order, but matching rpicam-apps keeps us on the well-trodden
  // path for this driver.

  // Pick a colorspace for the OUTPUT plane. If the producer told us
  // exactly which colorspace its frames carry (e.g. libcamera's
  // negotiated `Rec709` for >=720p), pass that through verbatim.
  // Otherwise default to `SMPTE170M` for SD and `REC709` for HD,
  // matching what rpicam-apps does for unannotated frames.
  uint32_t output_colorspace = input_colorspace_v4l2;
  if (output_colorspace == 0) {
    output_colorspace = (width >= 1280 || height >= 720)
                            ? V4L2_COLORSPACE_REC709
                            : V4L2_COLORSPACE_SMPTE170M;
  }

  v4l2_format fmt = {};
  fmt.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  fmt.fmt.pix_mp.width = width;
  fmt.fmt.pix_mp.height = height;
  fmt.fmt.pix_mp.pixelformat = input_fourcc_;
  // V4L2_FIELD_ANY on S_FMT lets the driver pick (rpicam-apps does the
  // same); V4L2_FIELD_NONE is set explicitly on every QBUF below so
  // bcm2835-codec never interprets a frame as interlaced.
  fmt.fmt.pix_mp.field = V4L2_FIELD_ANY;
  fmt.fmt.pix_mp.colorspace = output_colorspace;
  fmt.fmt.pix_mp.num_planes = 1;
  fmt.fmt.pix_mp.plane_fmt[0].bytesperline = output_stride_;
  fmt.fmt.pix_mp.plane_fmt[0].sizeimage = frame_size_;
  if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set output format: "
                      << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }
  const uint32_t negotiated_fourcc = fmt.fmt.pix_mp.pixelformat;
  if (negotiated_fourcc != input_fourcc_) {
    RTC_LOG(LS_WARNING)
        << "V4L2: Driver adjusted output pixel format from "
        << FourccToString(input_fourcc_) << " to "
        << FourccToString(negotiated_fourcc);
  }
  input_fourcc_ = negotiated_fourcc;
  output_stride_ = fmt.fmt.pix_mp.plane_fmt[0].bytesperline > 0
                       ? fmt.fmt.pix_mp.plane_fmt[0].bytesperline
                       : output_stride_;
  output_chroma_stride_ = ChromaStrideForFourcc(input_fourcc_, output_stride_);
  frame_size_ = std::max<int>(
      FrameSizeForFourcc(input_fourcc_, output_stride_, height),
      fmt.fmt.pix_mp.plane_fmt[0].sizeimage);
  output_luma_height_ = StorageLumaHeightForFourcc(
      input_fourcc_, output_stride_, height, frame_size_);
  output_chroma_height_ = ChromaHeight(output_luma_height_);
  if (output_luma_height_ != height) {
    RTC_LOG(LS_INFO)
        << "V4L2: using macroblock-aligned OUTPUT plane height "
        << output_luma_height_ << " for visible height " << height;
  }

  if (mode_ != OutputBufferMode::Dmabuf &&
      input_fourcc_ != V4L2_PIX_FMT_YUV420) {
    RTC_LOG(LS_ERROR)
        << "V4L2: Driver negotiated unsupported CPU-input pixel format "
        << FourccToString(input_fourcc_) << "; expected "
        << FourccToString(V4L2_PIX_FMT_YUV420);
    close(fd_);
    fd_ = -1;
    return false;
  }
  if (mode_ == OutputBufferMode::Dmabuf) {
    RTC_LOG(LS_INFO)
        << "V4L2: encoder input path confirmed: DMABUF zero-copy import "
        << "(memory DMABUF, fourcc " << FourccToString(input_fourcc_)
        << ", stride " << output_stride_ << ", sizeimage " << frame_size_
        << ")";
  } else if (mode_ == OutputBufferMode::Mmap) {
    RTC_LOG(LS_INFO)
        << "V4L2: encoder input path confirmed: CPU I420 copy into MMAP "
        << "(fourcc " << FourccToString(input_fourcc_) << ", stride "
        << output_stride_ << ", sizeimage " << frame_size_ << ")";
  } else {
    RTC_LOG(LS_INFO)
        << "V4L2: encoder input path confirmed: USERPTR input (fourcc "
        << FourccToString(input_fourcc_) << ", stride " << output_stride_
        << ", sizeimage " << frame_size_ << ")";
  }

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
  fmt.fmt.pix_mp.plane_fmt[0].sizeimage = capture_buffer_size_;
  if (Xioctl(fd_, VIDIOC_S_FMT, &fmt) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to set capture format: "
                      << strerror(errno);
    close(fd_);
    fd_ = -1;
    return false;
  }
  capture_buffer_size_ =
      std::max<int>(capture_buffer_size_, fmt.fmt.pix_mp.plane_fmt[0].sizeimage);

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
  RTC_LOG(LS_VERBOSE) << "V4L2: Allocated " << num_output_buffers_
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
  RTC_LOG(LS_VERBOSE) << "V4L2: Allocated " << num_capture_buffers_
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
    buf.field = V4L2_FIELD_NONE;
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

  RTC_LOG(LS_INFO) << "V4L2: H.264 encoder initialized -- " << width << "x"
                   << height << " @ " << framerate << " fps, "
                   << bitrate << " bps, mode " << ModeName(mode_)
                   << ", output stride " << output_stride_
                   << ", output sizeimage " << frame_size_
                   << ", capture sizeimage " << capture_buffer_size_;

  // Prime the pipeline in MMAP mode only; USERPTR/DMABUF priming would
  // require fabricating a contiguous user buffer or DMABUF, which the
  // caller would need to provide. The first user-submitted frame is
  // marked as IDR so the decoder still gets a clean start.
  //
  // PrimeEncoderPipeline does its own inline DQBUF, so it must run
  // before the poll thread is spawned to avoid a race on the V4L2 fd.
  if (mode_ == OutputBufferMode::Mmap) {
    PrimeEncoderPipeline();
  }

  // Spawn the poll thread that owns DQBUF on both queues. From now on
  // the encoder thread interacts with V4L2 only via QBUF + condvars.
  abort_poll_.store(false, std::memory_order_release);
  poll_thread_ = std::thread([this]() { PollThreadLoop(); });

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
  //   [Y plane] [U plane] [V plane], each using the negotiated V4L2 stride.
  // Source strides may differ from width, so we copy row-by-row.

  uint8_t* dst = static_cast<uint8_t*>(output_buffers_[index].start);
  const int dst_stride_y = output_stride_;
  const int dst_stride_uv = output_chroma_stride_;
  const int chroma_width = ChromaWidth(width_);
  const int chroma_height = ChromaHeight(height_);
  memset(dst, 0, output_buffers_[index].length);

  for (int row = 0; row < height_; row++)
    memcpy(dst + row * dst_stride_y, y + row * stride_y, width_);

  uint8_t* dst_u = dst + dst_stride_y * output_luma_height_;
  memset(dst_u, 128, dst_stride_uv * output_chroma_height_);
  for (int row = 0; row < chroma_height; row++)
    memcpy(dst_u + row * dst_stride_uv, u + row * stride_u, chroma_width);

  uint8_t* dst_v = dst_u + dst_stride_uv * output_chroma_height_;
  memset(dst_v, 128, dst_stride_uv * output_chroma_height_);
  for (int row = 0; row < chroma_height; row++)
    memcpy(dst_v + row * dst_stride_uv, v + row * stride_v, chroma_width);
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

  std::vector<uint8_t> black_frame(frame_size_, 0);

  // Build a proper black I420 frame: Y=0 (black luma), U=V=128 (neutral
  // chroma, i.e. no colour cast).
  const int y_size = output_stride_ * output_luma_height_;
  const int uv_size = output_chroma_stride_ * output_chroma_height_;
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
    buf.field = V4L2_FIELD_NONE;
    buf.length = 1;
    buf.m.planes = planes;
    buf.m.planes[0].bytesused = frame_size_;
    buf.m.planes[0].length = output_buffers_[idx].length;
    if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
      RTC_LOG(LS_WARNING) << "V4L2: Prime: QBUF output[" << idx
                          << "] failed: " << strerror(errno);
      break;
    }
    output_buffer_queued_[idx] = true;
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
    int dq_ret = Xioctl(fd_, VIDIOC_DQBUF, &buf);
    if (dq_ret < 0) {
      if (errno != EAGAIN)
        RTC_LOG(LS_WARNING) << "V4L2: Prime: DQBUF output failed: "
                            << strerror(errno);
    } else if (buf.index < kNumOutputBuffers) {
      output_buffer_queued_[buf.index] = false;
    }

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
    if (buf.index >= static_cast<uint32_t>(num_capture_buffers_)) {
      RTC_LOG(LS_WARNING)
          << "V4L2: Prime: ignoring CAPTURE buffer with invalid index "
          << buf.index;
      continue;
    }

    // Re-queue the capture buffer for future use.
    v4l2_buffer requeue = {};
    v4l2_plane rq_planes[VIDEO_MAX_PLANES] = {};
    requeue.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    requeue.memory = V4L2_MEMORY_MMAP;
    requeue.index = buf.index;
    requeue.field = V4L2_FIELD_NONE;
    requeue.length = 1;
    requeue.m.planes = rq_planes;
    requeue.m.planes[0].length = capture_buffers_[buf.index].length;
    Xioctl(fd_, VIDIOC_QBUF, &requeue);
  }

  // Reset so the first real Encode() call starts from buffer 0.
  next_output_index_ = 0;
  pending_frames_.clear();
  ready_frames_.clear();
  RTC_LOG(LS_INFO) << "V4L2: Encoder pipeline primed";
}

// ---------------------------------------------------------------------------
// Queue draining helpers
//
// All DQBUF on both queues happens on the dedicated poll thread (see
// PollThreadLoop). The encoder thread (calling Encode/EncodeDmabuf) only
// QBUFs and waits on condvars for state changes. This mirrors the
// rpicam-apps design (separate pollThread / outputThread) and keeps the
// WebRTC encoder thread off the V4L2 fd hot path.
// ---------------------------------------------------------------------------

bool V4l2H264EncoderWrapper::QueueCaptureBuffer(int index) {
  if (index < 0 || index >= num_capture_buffers_)
    return false;

  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  buf.memory = V4L2_MEMORY_MMAP;
  buf.index = index;
  buf.field = V4L2_FIELD_NONE;
  buf.length = 1;
  buf.m.planes = planes;
  buf.m.planes[0].length = capture_buffers_[index].length;
  if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: Failed to re-queue capture buffer "
                      << index << ": " << strerror(errno);
    return false;
  }
  return true;
}

void V4l2H264EncoderWrapper::DrainReadyOutputBuffers() {
  for (;;) {
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    buf.memory = V4l2MemoryFor(mode_);
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
      if (errno != EAGAIN && errno != EPIPE)
        RTC_LOG(LS_ERROR) << "V4L2: DQBUF output failed: " << strerror(errno);
      return;
    }
    if (buf.index < kNumOutputBuffers) {
      {
        std::lock_guard<std::mutex> lock(mutex_);
        output_buffer_queued_[buf.index] = false;
        retained_input_buffers_[buf.index] = nullptr;
      }
      output_buffer_cv_.notify_all();
    }
  }
}

void V4l2H264EncoderWrapper::DrainReadyCaptureBuffers() {
  for (;;) {
    v4l2_buffer buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    buf.memory = V4L2_MEMORY_MMAP;
    buf.length = 1;
    buf.m.planes = planes;
    if (Xioctl(fd_, VIDIOC_DQBUF, &buf) < 0) {
      if (errno != EAGAIN && errno != EPIPE)
        RTC_LOG(LS_ERROR) << "V4L2: DQBUF capture failed: " << strerror(errno);
      return;
    }
    if (buf.index >= static_cast<uint32_t>(num_capture_buffers_)) {
      RTC_LOG(LS_WARNING) << "V4L2: ignoring CAPTURE buffer with invalid index "
                          << buf.index;
      {
        std::lock_guard<std::mutex> lock(mutex_);
        force_next_keyframe_ = true;
      }
      continue;
    }

    const uint64_t timestamp_us = TimevalToUsec(buf.timestamp);
    PendingFrame pending;
    bool found_pending = false;
    {
      std::lock_guard<std::mutex> lock(mutex_);
      for (auto it = pending_frames_.begin(); it != pending_frames_.end(); ++it) {
        if (it->v4l2_timestamp_us == timestamp_us) {
          pending = *it;
          pending_frames_.erase(it);
          found_pending = true;
          break;
        }
      }
      if (!found_pending && !pending_frames_.empty()) {
        pending = pending_frames_.front();
        pending_frames_.pop_front();
        found_pending = true;
        RTC_LOG(LS_WARNING)
            << "V4L2: CAPTURE buffer timestamp did not match any pending "
               "OUTPUT buffer; using oldest pending frame";
      }
    }

    const size_t bytesused = buf.m.planes[0].bytesused;
    const size_t data_offset = buf.m.planes[0].data_offset;
    const bool capture_error = (buf.flags & V4L2_BUF_FLAG_ERROR) != 0;
    const bool invalid_size =
        bytesused <= data_offset || bytesused > capture_buffers_[buf.index].length;
    if (capture_error || invalid_size || !found_pending) {
      if (capture_error || invalid_size) {
        char flags[16];
        std::snprintf(flags, sizeof(flags), "0x%08x", buf.flags);
        RTC_LOG(LS_WARNING)
            << "V4L2: dropping invalid CAPTURE buffer"
            << " flags=" << flags
            << " bytesused=" << bytesused
            << " data_offset=" << data_offset
            << " length=" << capture_buffers_[buf.index].length;
        std::lock_guard<std::mutex> lock(mutex_);
        force_next_keyframe_ = true;
        if (found_pending && pending.requires_parameter_sets)
          require_next_keyframe_parameter_sets_ = true;
      }
      QueueCaptureBuffer(buf.index);
      continue;
    }

    const size_t encoded_size = bytesused - data_offset;
    auto* data = static_cast<const uint8_t*>(capture_buffers_[buf.index].start) +
                 data_offset;
    const H264AccessUnitInfo h264_info =
        InspectH264AccessUnit(data, encoded_size);
    const bool has_parameter_sets = h264_info.has_sps && h264_info.has_pps;
    const bool real_key_frame = h264_info.has_idr;
    if (pending.key_frame && !real_key_frame) {
      RTC_LOG(LS_WARNING)
          << "V4L2: dropping requested keyframe without IDR NAL";
      {
        std::lock_guard<std::mutex> lock(mutex_);
        force_next_keyframe_ = true;
        if (pending.requires_parameter_sets)
          require_next_keyframe_parameter_sets_ = true;
      }
      QueueCaptureBuffer(buf.index);
      continue;
    }
    if (pending.requires_parameter_sets && !has_parameter_sets) {
      RTC_LOG(LS_WARNING)
          << "V4L2: dropping post-initialization keyframe without SPS/PPS";
      {
        std::lock_guard<std::mutex> lock(mutex_);
        force_next_keyframe_ = true;
        require_next_keyframe_parameter_sets_ = true;
      }
      QueueCaptureBuffer(buf.index);
      continue;
    }

    EncodedFrame frame;
    frame.bitstream = webrtc::EncodedImageBuffer::Create(data, encoded_size);
    frame.rtp_timestamp = pending.rtp_timestamp;
    frame.key_frame = real_key_frame;
    {
      std::lock_guard<std::mutex> lock(mutex_);
      ready_frames_.push_back(std::move(frame));
    }
    encoded_frame_cv_.notify_all();

    QueueCaptureBuffer(buf.index);
  }
}

void V4l2H264EncoderWrapper::PollThreadLoop() {
  while (!abort_poll_.load(std::memory_order_acquire)) {
    pollfd pfd = {fd_, POLLIN, 0};
    int ret = poll(&pfd, 1, /*timeout_ms=*/200);
    if (abort_poll_.load(std::memory_order_acquire))
      break;
    if (ret < 0) {
      if (errno == EINTR)
        continue;
      RTC_LOG(LS_ERROR) << "V4L2: poll thread: poll failed: "
                        << strerror(errno);
      // Hand off to the encoder thread by waking any waiters; they will
      // observe the abort flag and bail.
      abort_poll_.store(true, std::memory_order_release);
      output_buffer_cv_.notify_all();
      encoded_frame_cv_.notify_all();
      return;
    }
    if (ret == 0)
      continue;  // 200ms tick, just loop and re-check abort
    if (pfd.revents & (POLLERR | POLLNVAL)) {
      RTC_LOG(LS_ERROR) << "V4L2: poll thread: poll reported error revents";
      abort_poll_.store(true, std::memory_order_release);
      output_buffer_cv_.notify_all();
      encoded_frame_cv_.notify_all();
      return;
    }
    // bcm2835-codec signals POLLIN whenever either queue has work to
    // dequeue. Drain both unconditionally; either DQBUF returns EAGAIN
    // immediately when its queue is idle.
    DrainReadyOutputBuffers();
    DrainReadyCaptureBuffers();
  }
}

int V4l2H264EncoderWrapper::AcquireOutputBuffer(int timeout_ms) {
  std::unique_lock<std::mutex> lock(mutex_);
  const auto deadline =
      std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);

  for (;;) {
    if (abort_poll_.load(std::memory_order_acquire)) {
      RTC_LOG(LS_ERROR) << "V4L2: poll thread aborted while acquiring OUTPUT";
      return -1;
    }
    for (int attempt = 0; attempt < num_output_buffers_; ++attempt) {
      const int index = (next_output_index_ + attempt) % num_output_buffers_;
      if (!output_buffer_queued_[index]) {
        next_output_index_ = (index + 1) % num_output_buffers_;
        return index;
      }
    }

    if (output_buffer_cv_.wait_until(lock, deadline) ==
        std::cv_status::timeout) {
      // One more pass after timeout to handle the case where the poll
      // thread freed a buffer just as we were timing out.
      for (int attempt = 0; attempt < num_output_buffers_; ++attempt) {
        const int index = (next_output_index_ + attempt) % num_output_buffers_;
        if (!output_buffer_queued_[index]) {
          next_output_index_ = (index + 1) % num_output_buffers_;
          return index;
        }
      }
      RTC_LOG(LS_ERROR) << "V4L2: timeout waiting for a free OUTPUT buffer";
      return -1;
    }
  }
}

EncodeResult V4l2H264EncoderWrapper::WaitForEncodedFrame(int timeout_ms) {
  std::unique_lock<std::mutex> lock(mutex_);
  const auto deadline =
      std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);

  for (;;) {
    if (!ready_frames_.empty()) {
      EncodedFrame frame = std::move(ready_frames_.front());
      ready_frames_.pop_front();
      return EncodeOk(std::move(frame));
    }
    if (abort_poll_.load(std::memory_order_acquire))
      return EncodeError();
    if (timeout_ms <= 0)
      return EncodeNoOutput();
    if (encoded_frame_cv_.wait_until(lock, deadline) ==
        std::cv_status::timeout) {
      if (!ready_frames_.empty()) {
        EncodedFrame frame = std::move(ready_frames_.front());
        ready_frames_.pop_front();
        return EncodeOk(std::move(frame));
      }
      return EncodeNoOutput();
    }
  }
}

bool V4l2H264EncoderWrapper::WaitForOutputBuffer(int index, int timeout_ms) {
  if (index < 0 || index >= kNumOutputBuffers)
    return false;

  std::unique_lock<std::mutex> lock(mutex_);
  const auto deadline =
      std::chrono::steady_clock::now() + std::chrono::milliseconds(timeout_ms);

  while (output_buffer_queued_[index]) {
    if (abort_poll_.load(std::memory_order_acquire))
      return false;
    if (output_buffer_cv_.wait_until(lock, deadline) ==
        std::cv_status::timeout) {
      if (!output_buffer_queued_[index])
        return true;
      RTC_LOG(LS_ERROR) << "V4L2: timeout waiting for OUTPUT buffer "
                        << index << " to be consumed";
      return false;
    }
  }
  return true;
}

// ---------------------------------------------------------------------------
// Encoding -- planar Y/U/V input (Mmap or UserPtr)
// ---------------------------------------------------------------------------

EncodeResult
V4l2H264EncoderWrapper::Encode(const uint8_t* y,
                                const uint8_t* u,
                                const uint8_t* v,
                                int stride_y,
                                int stride_u,
                                int stride_v,
                                bool force_idr,
                                uint32_t rtp_timestamp) {
  if (!initialized_) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode called on uninitialized encoder";
    return EncodeError();
  }
  if (mode_ == OutputBufferMode::Dmabuf) {
    RTC_LOG(LS_ERROR) << "V4L2: Encode() called on DMABUF-mode encoder; "
                         "use EncodeDmabuf instead";
    return EncodeError();
  }

  const int buf_index = AcquireOutputBuffer(/*timeout_ms=*/1000);
  if (buf_index < 0)
    return EncodeError();

  const uint8_t* userptr = nullptr;

  if (mode_ == OutputBufferMode::UserPtr) {
    // USERPTR works only when the input planes are arranged as a single
    // contiguous I420 buffer matching what the encoder expects:
    //   [Y: stride_y == width, height rows] [U: stride_u == width/2, chroma_height rows] [V: ...]
    const int dst_stride_y = output_stride_;
    const int dst_stride_uv = output_chroma_stride_;
    const int chroma_width = ChromaWidth(width_);
    const int chroma_height = ChromaHeight(height_);
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
      memset(dst, 0, scratch.size());
      for (int row = 0; row < height_; row++)
        memcpy(dst + row * dst_stride_y, y + row * stride_y, width_);
      uint8_t* dst_u = dst + dst_stride_y * output_luma_height_;
      memset(dst_u, 128, dst_stride_uv * output_chroma_height_);
      for (int row = 0; row < chroma_height; row++)
        memcpy(dst_u + row * dst_stride_uv, u + row * stride_u, chroma_width);
      uint8_t* dst_v = dst_u + dst_stride_uv * output_chroma_height_;
      memset(dst_v, 128, dst_stride_uv * output_chroma_height_);
      for (int row = 0; row < chroma_height; row++)
        memcpy(dst_v + row * dst_stride_uv, v + row * stride_v, chroma_width);
      userptr = dst;
    }
  } else {
    // MMAP: copy the caller's I420 frame into the mmap'd buffer.
    CopyI420ToOutputBuffer(buf_index, y, u, v, stride_y, stride_u, stride_v);
  }

  return RunEncode(buf_index, force_idr, userptr, /*dmabuf_fd=*/-1,
                   /*offset=*/0, /*length=*/0, rtp_timestamp,
                   /*retained_input_buffer=*/nullptr,
                   /*encoded_timeout_ms=*/1000,
                   /*wait_for_output_buffer=*/mode_ == OutputBufferMode::UserPtr);
}

// ---------------------------------------------------------------------------
// Encoding -- DMABUF input
// ---------------------------------------------------------------------------

EncodeResult
V4l2H264EncoderWrapper::EncodeDmabuf(int dmabuf_fd,
                                      size_t offset,
                                      size_t length,
                                      bool force_idr,
                                      uint32_t rtp_timestamp,
                                      webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
                                          retained_input_buffer) {
  if (!initialized_) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called on uninitialized encoder";
    return EncodeError();
  }
  if (mode_ != OutputBufferMode::Dmabuf) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called but encoder is in "
                      << ModeName(mode_) << " mode";
    return EncodeError();
  }
  if (dmabuf_fd < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: EncodeDmabuf called with invalid fd";
    return EncodeError();
  }

  const int buf_index = AcquireOutputBuffer(/*timeout_ms=*/1000);
  if (buf_index < 0)
    return EncodeError();

  return RunEncode(buf_index, force_idr, /*userptr=*/nullptr, dmabuf_fd, offset,
                   length == 0 ? static_cast<size_t>(frame_size_) : length,
                   rtp_timestamp, std::move(retained_input_buffer),
                   /*encoded_timeout_ms=*/0,
                   /*wait_for_output_buffer=*/false);
}

// ---------------------------------------------------------------------------
// Shared encode-submit/dequeue path
// ---------------------------------------------------------------------------

EncodeResult
V4l2H264EncoderWrapper::RunEncode(int buf_index,
                                   bool force_idr,
                                   const uint8_t* userptr,
                                   int dmabuf_fd,
                                   size_t offset,
                                   size_t length,
                                   uint32_t rtp_timestamp,
                                   webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
                                       retained_input_buffer,
                                   int encoded_timeout_ms,
                                   bool wait_for_output_buffer) {
  // Take the lock to consume force_next_keyframe_ atomically and to grab
  // a unique v4l2 timestamp for matching in DrainReadyCaptureBuffers.
  uint64_t v4l2_timestamp_us;
  bool requires_parameter_sets;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    force_idr = force_idr || force_next_keyframe_;
    requires_parameter_sets =
        force_idr && require_next_keyframe_parameter_sets_;
    v4l2_timestamp_us = next_v4l2_timestamp_us_++;
  }

  if (force_idr) {
    v4l2_control ctrl = {};
    ctrl.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
    ctrl.value = 1;
    if (Xioctl(fd_, VIDIOC_S_CTRL, &ctrl) < 0)
      RTC_LOG(LS_WARNING) << "V4L2: Failed to force IDR: " << strerror(errno);
  }

  // Build the OUTPUT QBUF descriptor.
  v4l2_buffer buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  buf.memory = V4l2MemoryFor(mode_);
  buf.index = buf_index;
  buf.field = V4L2_FIELD_NONE;
  buf.length = 1;
  buf.m.planes = planes;
  buf.m.planes[0].bytesused = static_cast<uint32_t>(
      mode_ == OutputBufferMode::Dmabuf ? length : frame_size_);
  buf.timestamp = TimevalFromUsec(v4l2_timestamp_us);

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

  // Mark the slot as queued and record the pending frame BEFORE QBUF so
  // the poll thread cannot dequeue and reuse the index between QBUF and
  // our state update. If QBUF fails we roll the bookkeeping back.
  {
    std::lock_guard<std::mutex> lock(mutex_);
    output_buffer_queued_[buf_index] = true;
    retained_input_buffers_[buf_index] = std::move(retained_input_buffer);
    pending_frames_.push_back(PendingFrame{
        v4l2_timestamp_us, rtp_timestamp, force_idr, requires_parameter_sets});
    if (force_idr) {
      force_next_keyframe_ = false;
      require_next_keyframe_parameter_sets_ = false;
    }
  }

  if (Xioctl(fd_, VIDIOC_QBUF, &buf) < 0) {
    RTC_LOG(LS_ERROR) << "V4L2: QBUF output failed (mode " << ModeName(mode_)
                      << "): " << strerror(errno);
    {
      std::lock_guard<std::mutex> lock(mutex_);
      output_buffer_queued_[buf_index] = false;
      retained_input_buffers_[buf_index] = nullptr;
      for (auto it = pending_frames_.begin(); it != pending_frames_.end(); ++it) {
        if (it->v4l2_timestamp_us == v4l2_timestamp_us) {
          pending_frames_.erase(it);
          break;
        }
      }
    }
    output_buffer_cv_.notify_all();
    return EncodeError();
  }

  EncodeResult result = WaitForEncodedFrame(encoded_timeout_ms);
  if (wait_for_output_buffer) {
    if (!WaitForOutputBuffer(buf_index, /*timeout_ms=*/1000)) {
      RTC_LOG(LS_ERROR) << "V4L2: " << ModeName(mode_)
                        << " input buffer still queued after timeout";
      return EncodeError();
    }
    if (result.status == EncodeResult::Status::NoOutput) {
      std::lock_guard<std::mutex> lock(mutex_);
      if (!ready_frames_.empty()) {
        EncodedFrame frame = std::move(ready_frames_.front());
        ready_frames_.pop_front();
        return EncodeOk(std::move(frame));
      }
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Runtime rate updates
// ---------------------------------------------------------------------------

void V4l2H264EncoderWrapper::UpdateRates(int framerate, int bitrate) {
  if (fd_ < 0)
    return;

  if (bitrate > 0 && bitrate != bitrate_) {
    RTC_LOG(LS_VERBOSE) << "V4L2: updating encoder bitrate from "
                        << bitrate_ << " to " << bitrate << " bps";
    bitrate_ = bitrate;
    TrySetControl(fd_, V4L2_CID_MPEG_VIDEO_BITRATE, bitrate, "bitrate");
  }

  if (framerate > 0 && framerate != framerate_) {
    RTC_LOG(LS_VERBOSE) << "V4L2: updating encoder framerate from "
                        << framerate_ << " to " << framerate << " fps";
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

  // 1. Stop the poll thread first so we own the V4L2 fd exclusively
  //    when issuing STREAMOFF / REQBUFS(0). Joining within ~200 ms
  //    (the poll() timeout in PollThreadLoop) is acceptable for
  //    Release()/Destroy() paths.
  abort_poll_.store(true, std::memory_order_release);
  output_buffer_cv_.notify_all();
  encoded_frame_cv_.notify_all();
  if (poll_thread_.joinable())
    poll_thread_.join();

  // 2. Stop both streaming queues.
  v4l2_buf_type type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);
  type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  Xioctl(fd_, VIDIOC_STREAMOFF, &type);

  // 3. Unmap and release OUTPUT buffers (only mapped in Mmap mode).
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

  // 4. Unmap and release CAPTURE buffers.
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

  // 5. Close the device.
  close(fd_);
  fd_ = -1;
  num_output_buffers_ = 0;
  num_capture_buffers_ = 0;
  pending_frames_.clear();
  ready_frames_.clear();
  for (int i = 0; i < kNumOutputBuffers; ++i)
    output_buffer_queued_[i] = false;
  for (int i = 0; i < kNumOutputBuffers; ++i)
    retained_input_buffers_[i] = nullptr;
  initialized_ = false;

  RTC_LOG(LS_INFO) << "V4L2: Encoder destroyed";
}

}  // namespace livekit_ffi
