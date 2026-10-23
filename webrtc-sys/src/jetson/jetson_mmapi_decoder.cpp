/*
 * Copyright 2026 LiveKit, Inc.
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

#include "jetson_mmapi_decoder.h"

#include <fcntl.h>
#include <pthread.h>
#include <sys/stat.h>
#include <unistd.h>

#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <memory>

#include "NvBufSurface.h"
#include "NvVideoDecoder.h"
#include "jetson_runtime_loader.h"
#include "rtc_base/logging.h"

namespace {

// Input buffers hold one complete access unit each. 4 MiB covers 4K
// keyframes at WebRTC bitrates; a larger access unit is rejected.
constexpr uint32_t kOutputBufferSize = 4 * 1024 * 1024;
constexpr uint32_t kOutputBufferCount = 4;
// Pictures the capture thread can hold beyond the decoder's minimum.
constexpr int kExtraCaptureBuffers = 2;
// The device is non-blocking, so both threads poll. A 1 ms interval bounds
// the added latency while keeping Destroy() able to stop the capture thread.
constexpr useconds_t kPollIntervalUs = 1000;
constexpr int kOutputBufferWaitMs = 200;
// Keeps the decoder's reference picture buffer enabled. Disabling it removes
// a frame of latency but corrupts streams that use B-frames.
constexpr char kKeepDpbEnvVar[] = "LK_JETSON_DECODER_KEEP_DPB";

bool DeviceExists(const char* path) {
  struct stat st = {};
  return stat(path, &st) == 0;
}

bool DecoderDeviceExists() {
  return DeviceExists("/dev/nvhost-nvdec") || DeviceExists("/dev/v4l2-nvdec");
}

// NVIDIA's own fourcc first, as in the MMAPI decode sample, then the
// upstream V4L2 one.
bool SetBitstreamFormat(NvVideoDecoder* decoder, livekit::JetsonCodec codec) {
  switch (codec) {
    case livekit::JetsonCodec::kH264:
      return decoder->setOutputPlaneFormat(V4L2_PIX_FMT_H264,
                                           kOutputBufferSize) == 0;
    case livekit::JetsonCodec::kH265:
      return decoder->setOutputPlaneFormat(V4L2_PIX_FMT_H265,
                                           kOutputBufferSize) == 0 ||
             decoder->setOutputPlaneFormat(V4L2_PIX_FMT_HEVC,
                                           kOutputBufferSize) == 0;
    case livekit::JetsonCodec::kAV1:
      return decoder->setOutputPlaneFormat(V4L2_PIX_FMT_AV1,
                                           kOutputBufferSize) == 0;
  }
  return false;
}

// V4L2 copies the input buffer's timestamp to the picture decoded from it,
// which is how a picture is matched back to its RTP timestamp.
struct timeval RtpToTimeval(uint32_t rtp_timestamp) {
  struct timeval tv = {};
  tv.tv_sec = rtp_timestamp / 1000000;
  tv.tv_usec = rtp_timestamp % 1000000;
  return tv;
}

uint32_t TimevalToRtp(const struct timeval& tv) {
  return static_cast<uint32_t>(tv.tv_sec) * 1000000u +
         static_cast<uint32_t>(tv.tv_usec);
}

}  // namespace

namespace livekit {

JetsonMmapiDecoder::JetsonMmapiDecoder(JetsonCodec codec) : codec_(codec) {}

JetsonMmapiDecoder::~JetsonMmapiDecoder() {
  Destroy();
}

bool JetsonMmapiDecoder::IsCodecSupported(JetsonCodec codec) {
  if (!DecoderDeviceExists()) {
    return false;
  }
  // The MMAPI/NvBufSurface symbols below are lazily bound and abort the
  // process if invoked while the Jetson userspace libraries are absent.
  if (!lk_jetson_decoder_runtime_libs_available()) {
    RTC_LOG(LS_WARNING) << "Jetson decoder device exists, but the Jetson "
                           "multimedia libraries could not be loaded.";
    return false;
  }
  std::unique_ptr<NvVideoDecoder> decoder(
      NvVideoDecoder::createVideoDecoder("livekit-decoder-probe", O_NONBLOCK));
  return decoder && SetBitstreamFormat(decoder.get(), codec);
}

bool JetsonMmapiDecoder::Initialize(FrameCallback callback) {
  Destroy();
  if (!lk_jetson_decoder_runtime_libs_available()) {
    return false;
  }

  decoder_ =
      NvVideoDecoder::createVideoDecoder("livekit-decoder", O_NONBLOCK);
  if (!decoder_) {
    RTC_LOG(LS_ERROR) << "Failed to create NvVideoDecoder.";
    return false;
  }

  bool ok = decoder_->subscribeEvent(V4L2_EVENT_RESOLUTION_CHANGE, 0, 0) == 0 &&
            SetBitstreamFormat(decoder_, codec_) &&
            // 0 keeps complete-frame input enabled. WebRTC hands over whole
            // access units, so the decoder need not wait for the start of the
            // next one before decoding the current one.
            decoder_->setFrameInputMode(0) == 0;
  if (ok && std::getenv(kKeepDpbEnvVar) == nullptr &&
      decoder_->disableDPB() != 0) {
    RTC_LOG(LS_WARNING) << "Jetson decoder could not disable its DPB; "
                           "decoding with an extra frame of latency.";
  }
  if (ok && decoder_->setMaxPerfMode(1) != 0) {
    RTC_LOG(LS_WARNING) << "Jetson decoder could not enable max perf mode.";
  }
  ok = ok &&
       decoder_->output_plane.setupPlane(V4L2_MEMORY_MMAP, kOutputBufferCount,
                                         true, false) == 0 &&
       decoder_->output_plane.setStreamStatus(true) == 0;
  if (!ok) {
    RTC_LOG(LS_ERROR) << "Failed to configure the Jetson decoder.";
    Destroy();
    return false;
  }

  callback_ = std::move(callback);
  stop_ = false;
  failed_ = false;
  output_buffers_used_ = 0;
  capture_thread_ = std::thread([this] { CaptureLoop(); });
  RTC_LOG(LS_INFO) << "Jetson MMAPI decoder initialized.";
  return true;
}

void JetsonMmapiDecoder::Destroy() {
  stop_ = true;
  if (capture_thread_.joinable()) {
    capture_thread_.join();
  }
  if (decoder_) {
    // Stops both planes; must follow the join so the capture thread never
    // touches a plane that is being torn down.
    decoder_->output_plane.setStreamStatus(false);
    ReleaseCapturePlane();
    decoder_->output_plane.deinitPlane();
    delete decoder_;
    decoder_ = nullptr;
  }
  callback_ = nullptr;
}

bool JetsonMmapiDecoder::Decode(const uint8_t* data,
                                size_t size,
                                uint32_t rtp_timestamp) {
  if (!decoder_ || failed_ || size == 0) {
    return false;
  }
  // Checked before taking an input buffer, which would otherwise be lost.
  if (size > kOutputBufferSize) {
    RTC_LOG(LS_ERROR) << "Access unit of " << size
                      << " bytes does not fit the Jetson decoder input.";
    return false;
  }

  struct v4l2_buffer v4l2_buf = {};
  struct v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.m.planes = planes;
  NvBuffer* buffer = nullptr;

  if (output_buffers_used_ < decoder_->output_plane.getNumBuffers()) {
    // Never queued yet: use it directly.
    v4l2_buf.index = output_buffers_used_++;
    buffer = decoder_->output_plane.getNthBuffer(v4l2_buf.index);
  } else {
    // Reuse an input buffer the decoder has consumed.
    int waited_ms = 0;
    while (decoder_->output_plane.dqBuffer(v4l2_buf, &buffer, nullptr, 0) !=
           0) {
      if (errno != EAGAIN || decoder_->isInError()) {
        failed_ = true;
        RTC_LOG(LS_ERROR) << "Jetson decoder failed to dequeue an input "
                             "buffer.";
        return false;
      }
      if (++waited_ms > kOutputBufferWaitMs) {
        RTC_LOG(LS_WARNING) << "Jetson decoder input is backed up; dropping "
                               "an access unit.";
        return false;
      }
      usleep(kPollIntervalUs);
    }
  }

  if (!buffer) {
    failed_ = true;
    return false;
  }

  std::memcpy(buffer->planes[0].data, data, size);
  buffer->planes[0].bytesused = static_cast<uint32_t>(size);
  v4l2_buf.m.planes[0].bytesused = static_cast<uint32_t>(size);
  v4l2_buf.flags |= V4L2_BUF_FLAG_TIMESTAMP_COPY;
  v4l2_buf.timestamp = RtpToTimeval(rtp_timestamp);
  if (decoder_->output_plane.qBuffer(v4l2_buf, nullptr) != 0) {
    failed_ = true;
    RTC_LOG(LS_ERROR) << "Jetson decoder failed to queue an access unit.";
    return false;
  }
  return true;
}

void JetsonMmapiDecoder::CaptureLoop() {
  pthread_setname_np(pthread_self(), "lk-jetson-dec");

  while (!stop_) {
    if (!HandleCaptureEvents()) {
      failed_ = true;
      return;
    }
    if (!capture_ready_) {
      usleep(kPollIntervalUs);
      continue;
    }

    switch (DeliverNextPicture(/*requeue=*/true)) {
      case PictureResult::kDelivered:
        break;
      case PictureResult::kNone:
        usleep(kPollIntervalUs);
        break;
      case PictureResult::kFailed:
        failed_ = true;
        return;
    }
  }
}

JetsonMmapiDecoder::PictureResult JetsonMmapiDecoder::DeliverNextPicture(
    bool requeue) {
  struct v4l2_buffer v4l2_buf = {};
  struct v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.m.planes = planes;
  NvBuffer* buffer = nullptr;
  if (decoder_->capture_plane.dqBuffer(v4l2_buf, &buffer, nullptr, 0) != 0) {
    if (errno != EAGAIN || decoder_->isInError()) {
      RTC_LOG(LS_ERROR) << "Jetson decoder failed to dequeue a picture.";
      return PictureResult::kFailed;
    }
    return PictureResult::kNone;
  }

  // A picture the decoder had to drop comes back without payload.
  if (buffer->planes[0].bytesused > 0 &&
      !DeliverFrame(buffer->planes[0].fd, TimevalToRtp(v4l2_buf.timestamp))) {
    RTC_LOG(LS_WARNING) << "Jetson decoder dropped a picture it could not "
                           "convert.";
  }
  if (requeue && decoder_->capture_plane.qBuffer(v4l2_buf, nullptr) != 0) {
    RTC_LOG(LS_ERROR) << "Jetson decoder failed to requeue a picture buffer.";
    return PictureResult::kFailed;
  }
  return PictureResult::kDelivered;
}

bool JetsonMmapiDecoder::HandleCaptureEvents() {
  struct v4l2_event event = {};
  // Non-blocking: returns at once when no event is pending.
  while (decoder_->dqEvent(event, 0) == 0) {
    if (event.type != V4L2_EVENT_RESOLUTION_CHANGE) {
      continue;
    }
    // Pictures decoded at the old size are still queued; deliver them
    // before their buffers are freed.
    while (capture_ready_) {
      const PictureResult result = DeliverNextPicture(/*requeue=*/false);
      if (result == PictureResult::kFailed) {
        return false;
      }
      if (result == PictureResult::kNone) {
        break;
      }
    }
    if (!SetupCapturePlane()) {
      return false;
    }
  }
  return !decoder_->isInError();
}

bool JetsonMmapiDecoder::SetupCapturePlane() {
  struct v4l2_format format = {};
  struct v4l2_crop crop = {};
  if (decoder_->capture_plane.getFormat(format) != 0 ||
      decoder_->capture_plane.getCrop(crop) != 0) {
    RTC_LOG(LS_ERROR) << "Jetson decoder could not read the stream format.";
    return false;
  }

  ReleaseCapturePlane();

  int min_buffers = 0;
  if (decoder_->setCapturePlaneFormat(format.fmt.pix_mp.pixelformat,
                                      format.fmt.pix_mp.width,
                                      format.fmt.pix_mp.height) != 0 ||
      decoder_->getMinimumCapturePlaneBuffers(min_buffers) != 0 ||
      decoder_->capture_plane.setupPlane(V4L2_MEMORY_MMAP,
                                         min_buffers + kExtraCaptureBuffers,
                                         false, false) != 0 ||
      decoder_->capture_plane.setStreamStatus(true) != 0) {
    RTC_LOG(LS_ERROR) << "Jetson decoder could not set up its picture "
                         "buffers.";
    return false;
  }

  for (uint32_t i = 0; i < decoder_->capture_plane.getNumBuffers(); ++i) {
    struct v4l2_buffer v4l2_buf = {};
    struct v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    v4l2_buf.index = i;
    v4l2_buf.m.planes = planes;
    v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    v4l2_buf.memory = V4L2_MEMORY_MMAP;
    if (decoder_->capture_plane.qBuffer(v4l2_buf, nullptr) != 0) {
      RTC_LOG(LS_ERROR) << "Jetson decoder could not queue a picture buffer.";
      return false;
    }
  }

  // Decoded pictures are block-linear; they are converted into this
  // pitch-linear I420 surface, which stays mapped for CPU reads.
  width_ = static_cast<int>(crop.c.width);
  height_ = static_cast<int>(crop.c.height);
  NvBufSurf::NvCommonAllocateParams params = {};
  params.width = crop.c.width;
  params.height = crop.c.height;
  params.colorFormat = NVBUF_COLOR_FORMAT_YUV420;
  params.layout = NVBUF_LAYOUT_PITCH;
  params.memType = NVBUF_MEM_SURFACE_ARRAY;
  params.memtag = NvBufSurfaceTag_VIDEO_CONVERT;
  if (NvBufSurf::NvAllocate(&params, 1, &converted_fd_) != 0 ||
      NvBufSurfaceFromFd(converted_fd_,
                         reinterpret_cast<void**>(&converted_surface_)) != 0 ||
      NvBufSurfaceMap(converted_surface_, 0, -1, NVBUF_MAP_READ) != 0) {
    RTC_LOG(LS_ERROR) << "Jetson decoder could not allocate its conversion "
                         "surface.";
    ReleaseCapturePlane();
    return false;
  }

  capture_ready_ = true;
  RTC_LOG(LS_INFO) << "Jetson decoder stream is " << width_ << "x" << height_
                   << " with " << decoder_->capture_plane.getNumBuffers()
                   << " picture buffers.";
  return true;
}

void JetsonMmapiDecoder::ReleaseCapturePlane() {
  capture_ready_ = false;
  if (converted_surface_) {
    NvBufSurfaceUnMap(converted_surface_, 0, -1);
    converted_surface_ = nullptr;
  }
  if (converted_fd_ >= 0) {
    NvBufSurf::NvDestroy(converted_fd_);
    converted_fd_ = -1;
  }
  if (decoder_) {
    // Stops streaming, unmaps the buffers, and frees them.
    decoder_->capture_plane.deinitPlane();
  }
}

bool JetsonMmapiDecoder::DeliverFrame(int capture_fd, uint32_t rtp_timestamp) {
  NvBufSurf::NvCommonTransformParams transform = {};
  transform.src_width = static_cast<uint32_t>(width_);
  transform.src_height = static_cast<uint32_t>(height_);
  transform.dst_width = static_cast<uint32_t>(width_);
  transform.dst_height = static_cast<uint32_t>(height_);
  transform.flag = NVBUFSURF_TRANSFORM_FILTER;
  transform.flip = NvBufSurfTransform_None;
  transform.filter = NvBufSurfTransformInter_Nearest;
  if (NvBufSurf::NvTransform(&transform, capture_fd, converted_fd_) != 0 ||
      NvBufSurfaceSyncForCpu(converted_surface_, 0, -1) != 0) {
    return false;
  }

  const NvBufSurfaceParams& surface = converted_surface_->surfaceList[0];
  JetsonDecodedFrame frame;
  frame.data_y = static_cast<const uint8_t*>(surface.mappedAddr.addr[0]);
  frame.stride_y = static_cast<int>(surface.planeParams.pitch[0]);
  frame.data_u = static_cast<const uint8_t*>(surface.mappedAddr.addr[1]);
  frame.stride_u = static_cast<int>(surface.planeParams.pitch[1]);
  frame.data_v = static_cast<const uint8_t*>(surface.mappedAddr.addr[2]);
  frame.stride_v = static_cast<int>(surface.planeParams.pitch[2]);
  frame.width = width_;
  frame.height = height_;
  frame.rtp_timestamp = rtp_timestamp;
  callback_(frame);
  return true;
}

}  // namespace livekit
