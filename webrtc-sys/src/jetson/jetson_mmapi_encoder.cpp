#include "jetson_mmapi_encoder.h"

#include <cerrno>
#include <linux/v4l2-controls.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <memory>
#include <thread>

#include "NvBufSurface.h"
#include "NvBuffer.h"
#include "NvVideoEncoder.h"
#include "NvUtils.h"
#include "rtc_base/logging.h"

namespace {

constexpr int kDefaultOutputBufferCount = 4;
constexpr int kDefaultCaptureBufferCount = 4;
constexpr int kMinBitstreamBufferSize = 1024 * 1024;

bool DeviceExists(const char* path) {
  struct stat st = {};
  return stat(path, &st) == 0;
}

void CopyPlane(uint8_t* dst,
               int dst_stride,
               const uint8_t* src,
               int src_stride,
               int width,
               int height) {
  for (int y = 0; y < height; ++y) {
    std::memcpy(dst + y * dst_stride, src + y * src_stride, width);
  }
}

}  // namespace

namespace livekit {

JetsonMmapiEncoder::JetsonMmapiEncoder(JetsonCodec codec) : codec_(codec) {}

JetsonMmapiEncoder::~JetsonMmapiEncoder() {
  Destroy();
}

bool JetsonMmapiEncoder::IsSupported() {
  return IsCodecSupported(JetsonCodec::kH264) ||
         IsCodecSupported(JetsonCodec::kH265);
}

bool JetsonMmapiEncoder::IsCodecSupported(JetsonCodec codec) {
  auto device = FindEncoderDevice();
  if (!device.has_value()) {
    return false;
  }
  std::unique_ptr<NvVideoEncoder> encoder(
      NvVideoEncoder::createVideoEncoder("livekit-encoder"));
  if (!encoder) {
    return false;
  }
  const uint32_t pixfmt = CodecToV4L2PixFmt(codec);
  if (encoder->setCapturePlaneFormat(pixfmt, 64, 64,
                                     kMinBitstreamBufferSize) < 0) {
    return false;
  }
  return true;
}

std::optional<std::string> JetsonMmapiEncoder::FindEncoderDevice() {
  if (DeviceExists("/dev/nvhost-msenc")) {
    RTC_LOG(LS_INFO) << "Jetson MMAPI encoder device: /dev/nvhost-msenc";
    return std::string("/dev/nvhost-msenc");
  }
  if (DeviceExists("/dev/v4l2-nvenc")) {
    RTC_LOG(LS_INFO) << "Jetson MMAPI encoder device: /dev/v4l2-nvenc";
    return std::string("/dev/v4l2-nvenc");
  }
  RTC_LOG(LS_WARNING) << "Jetson MMAPI encoder device not found.";
  return std::nullopt;
}

uint32_t JetsonMmapiEncoder::CodecToV4L2PixFmt(JetsonCodec codec) {
  return codec == JetsonCodec::kH264 ? V4L2_PIX_FMT_H264
                                     : V4L2_PIX_FMT_HEVC;
}

bool JetsonMmapiEncoder::Initialize(int width,
                                    int height,
                                    int framerate,
                                    int bitrate_bps,
                                    int keyframe_interval) {
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] Initialize called: %dx%d @ %d fps, bitrate=%d bps, "
                 "keyframe_interval=%d\n",
                 width, height, framerate, bitrate_bps, keyframe_interval);
    std::fflush(stderr);
  }

  if (initialized_) {
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] Already initialized, returning true\n");
      std::fflush(stderr);
    }
    return true;
  }

  width_ = width;
  height_ = height;
  framerate_ = framerate;
  bitrate_bps_ = bitrate_bps;
  keyframe_interval_ = keyframe_interval;

  auto device = FindEncoderDevice();
  if (!device.has_value()) {
    RTC_LOG(LS_WARNING) << "Jetson MMAPI encoder device not found.";
    std::fprintf(stderr, "[MMAPI] ERROR: Encoder device not found\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] Found encoder device: %s\n",
                 device->c_str());
    std::fflush(stderr);
  }

  if (!CreateEncoder()) {
    std::fprintf(stderr, "[MMAPI] ERROR: CreateEncoder() failed\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] CreateEncoder() succeeded\n");
    std::fflush(stderr);
  }

  if (!ConfigureEncoder()) {
    std::fprintf(stderr, "[MMAPI] ERROR: ConfigureEncoder() failed\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] ConfigureEncoder() succeeded (output_is_nv12=%d, "
                 "y_stride=%d, u_stride=%d, v_stride=%d)\n",
                 output_is_nv12_ ? 1 : 0, output_y_stride_, output_u_stride_,
                 output_v_stride_);
    std::fflush(stderr);
  }

  if (!SetupPlanes()) {
    std::fprintf(stderr, "[MMAPI] ERROR: SetupPlanes() failed\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] SetupPlanes() succeeded (output_buffers=%d, "
                 "capture_buffers=%d)\n",
                 output_buffer_count_, capture_buffer_count_);
    std::fflush(stderr);
  }

  if (!QueueCaptureBuffers()) {
    std::fprintf(stderr, "[MMAPI] ERROR: QueueCaptureBuffers() failed\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] QueueCaptureBuffers() succeeded\n");
    std::fflush(stderr);
  }

  if (!StartStreaming()) {
    std::fprintf(stderr, "[MMAPI] ERROR: StartStreaming() failed\n");
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] StartStreaming() succeeded\n");
    std::fflush(stderr);
  }

  initialized_ = true;
  std::fprintf(stderr,
               "[MMAPI] Encoder initialized successfully: %dx%d @ %d fps\n",
               width_, height_, framerate_);
  std::fflush(stderr);
  return true;
}

void JetsonMmapiEncoder::Destroy() {
  StopStreaming();
  if (encoder_) {
    delete encoder_;
    encoder_ = nullptr;
  }
  initialized_ = false;
}

bool JetsonMmapiEncoder::IsInitialized() const {
  return initialized_;
}

bool JetsonMmapiEncoder::Encode(const uint8_t* src_y,
                                int stride_y,
                                const uint8_t* src_u,
                                int stride_u,
                                const uint8_t* src_v,
                                int stride_v,
                                bool force_keyframe,
                                std::vector<uint8_t>* encoded,
                                bool* is_keyframe) {
  static std::atomic<uint64_t> encode_count(0);
  static std::atomic<uint64_t> success_count(0);
  static std::atomic<uint64_t> fail_count(0);
  static std::atomic<bool> logged_first_encode(false);
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  const uint64_t frame_num = encode_count.fetch_add(1);

  if (!initialized_ || !encoder_) {
    if (verbose || frame_num < 5) {
      std::fprintf(stderr,
                   "[MMAPI] Encode() called but encoder not initialized "
                   "(initialized=%d, encoder=%p)\n",
                   initialized_ ? 1 : 0, static_cast<void*>(encoder_));
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!logged_first_encode.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] First Encode() call: stride_y=%d, stride_u=%d, "
                 "stride_v=%d, force_keyframe=%d\n",
                 stride_y, stride_u, stride_v, force_keyframe ? 1 : 0);
    std::fflush(stderr);
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] ForceKeyframe() failed\n");
      std::fflush(stderr);
    }
  }

  if (!QueueOutputBuffer(src_y, stride_y, src_u, stride_u, src_v, stride_v)) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr, "[MMAPI] QueueOutputBuffer() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] DequeueCaptureBuffer() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!DequeueOutputBuffer()) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr, "[MMAPI] DequeueOutputBuffer() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  success_count.fetch_add(1);
  if (verbose && (frame_num < 5 || frame_num % 100 == 0)) {
    std::fprintf(stderr,
                 "[MMAPI] Encode() succeeded (frame %lu, encoded_size=%zu, "
                 "keyframe=%d, success=%lu, fail=%lu)\n",
                 frame_num, encoded->size(), is_keyframe ? *is_keyframe : -1,
                 success_count.load(), fail_count.load());
    std::fflush(stderr);
  }

  return true;
}

bool JetsonMmapiEncoder::EncodeNV12(const uint8_t* src_y,
                                    int stride_y,
                                    const uint8_t* src_uv,
                                    int stride_uv,
                                    bool force_keyframe,
                                    std::vector<uint8_t>* encoded,
                                    bool* is_keyframe) {
  static std::atomic<uint64_t> encode_count(0);
  static std::atomic<uint64_t> success_count(0);
  static std::atomic<uint64_t> fail_count(0);
  static std::atomic<bool> logged_first_encode(false);
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  const uint64_t frame_num = encode_count.fetch_add(1);

  if (!initialized_ || !encoder_) {
    if (verbose || frame_num < 5) {
      std::fprintf(stderr,
                   "[MMAPI] EncodeNV12() called but encoder not initialized "
                   "(initialized=%d, encoder=%p)\n",
                   initialized_ ? 1 : 0, static_cast<void*>(encoder_));
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!logged_first_encode.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] First EncodeNV12() call: stride_y=%d, stride_uv=%d, "
                 "force_keyframe=%d\n",
                 stride_y, stride_uv, force_keyframe ? 1 : 0);
    std::fflush(stderr);
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] ForceKeyframe() failed\n");
      std::fflush(stderr);
    }
  }

  if (!QueueOutputBufferNV12(src_y, stride_y, src_uv, stride_uv)) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBufferNV12() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] DequeueCaptureBuffer() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  if (!DequeueOutputBuffer()) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] DequeueOutputBuffer() failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  success_count.fetch_add(1);
  if (verbose && (frame_num < 5 || frame_num % 100 == 0)) {
    std::fprintf(stderr,
                 "[MMAPI] EncodeNV12() succeeded (frame %lu, encoded_size=%zu, "
                 "keyframe=%d, success=%lu, fail=%lu)\n",
                 frame_num, encoded->size(), is_keyframe ? *is_keyframe : -1,
                 success_count.load(), fail_count.load());
    std::fflush(stderr);
  }

  return true;
}

void JetsonMmapiEncoder::SetRates(int framerate, int bitrate_bps) {
  framerate_ = framerate;
  bitrate_bps_ = bitrate_bps;
  if (!encoder_) {
    return;
  }
  encoder_->setFrameRate(framerate_, 1);
  encoder_->setBitrate(bitrate_bps_);
}

void JetsonMmapiEncoder::SetKeyframeInterval(int keyframe_interval) {
  keyframe_interval_ = keyframe_interval;
  if (!encoder_) {
    return;
  }
  encoder_->setIDRInterval(keyframe_interval_);
  encoder_->setIFrameInterval(keyframe_interval_);
}

bool JetsonMmapiEncoder::CreateEncoder() {
  encoder_ = NvVideoEncoder::createVideoEncoder("livekit-encoder");
  if (!encoder_) {
    RTC_LOG(LS_ERROR) << "Failed to create NvVideoEncoder.";
    return false;
  }
  return true;
}

bool JetsonMmapiEncoder::ConfigureEncoder() {
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  const uint32_t codec_pixfmt = CodecToV4L2PixFmt(codec_);
  const uint32_t bitstream_size =
      std::max(kMinBitstreamBufferSize, width_ * height_);

  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] ConfigureEncoder: codec=%s, pixfmt=0x%x, "
                 "bitstream_size=%u\n",
                 codec_ == JetsonCodec::kH264 ? "H264" : "H265", codec_pixfmt,
                 bitstream_size);
    std::fflush(stderr);
  }

  // Set capture plane (encoded bitstream) first so the driver knows codec.
  int ret = encoder_->setCapturePlaneFormat(codec_pixfmt, width_, height_,
                                            bitstream_size);
  if (ret < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set capture plane format.";
    std::fprintf(stderr,
                 "[MMAPI] setCapturePlaneFormat failed: ret=%d, errno=%d "
                 "(%s)\n",
                 ret, errno, strerror(errno));
    std::fflush(stderr);
    return false;
  }
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setCapturePlaneFormat succeeded\n");
    std::fflush(stderr);
  }

  // Prefer planar YUV420 (I420-style) for Jetson end-to-end.
  // If that fails, fall back to NV12M. The I420 input path can still be used
  // with NV12 by interleaving U/V into UV in QueueOutputBuffer().
  output_is_nv12_ = false;
  ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_YUV420M, width_, height_);
  if (ret < 0) {
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] YUV420M format failed (ret=%d), trying NV12M\n",
                   ret);
      std::fflush(stderr);
    }
    ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_, height_);
    if (ret < 0) {
      RTC_LOG(LS_ERROR) << "Failed to set output plane format.";
      std::fprintf(stderr,
                   "[MMAPI] setOutputPlaneFormat failed for both YUV420M and "
                   "NV12M: ret=%d, errno=%d (%s)\n",
                   ret, errno, strerror(errno));
      std::fflush(stderr);
      return false;
    }
    output_is_nv12_ = true;
  }
  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] setOutputPlaneFormat succeeded (is_nv12=%d)\n",
                 output_is_nv12_ ? 1 : 0);
    std::fflush(stderr);
  }

  // Set encoder parameters and log results
  ret = encoder_->setBitrate(bitrate_bps_);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setBitrate(%d): ret=%d\n", bitrate_bps_, ret);
    std::fflush(stderr);
  }

  ret = encoder_->setFrameRate(framerate_, 1);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setFrameRate(%d, 1): ret=%d\n", framerate_,
                 ret);
    std::fflush(stderr);
  }

  ret = encoder_->setRateControlMode(V4L2_MPEG_VIDEO_BITRATE_MODE_CBR);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setRateControlMode(CBR): ret=%d\n", ret);
    std::fflush(stderr);
  }

  ret = encoder_->setIDRInterval(keyframe_interval_);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setIDRInterval(%d): ret=%d\n",
                 keyframe_interval_, ret);
    std::fflush(stderr);
  }

  ret = encoder_->setIFrameInterval(keyframe_interval_);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setIFrameInterval(%d): ret=%d\n",
                 keyframe_interval_, ret);
    std::fflush(stderr);
  }

  ret = encoder_->setInsertSpsPpsAtIdrEnabled(true);
  if (verbose) {
    std::fprintf(stderr, "[MMAPI] setInsertSpsPpsAtIdrEnabled(true): ret=%d\n",
                 ret);
    std::fflush(stderr);
  }

  if (codec_ == JetsonCodec::kH264) {
    ret = encoder_->setProfile(V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE);
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] setProfile(BASELINE): ret=%d\n", ret);
      std::fflush(stderr);
    }
    // Match the factory-advertised SDP profile-level-id (42e01f == CBP L3.1)
    // to avoid decoders rejecting due to an SPS level_idc higher than SDP.
    ret = encoder_->setLevel(V4L2_MPEG_VIDEO_H264_LEVEL_3_1);
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] setLevel(3.1): ret=%d\n", ret);
      std::fflush(stderr);
    }
  } else {
    ret = encoder_->setProfile(V4L2_MPEG_VIDEO_H265_PROFILE_MAIN);
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] setProfile(MAIN): ret=%d\n", ret);
      std::fflush(stderr);
    }
  }

  v4l2_format output_format = {};
  ret = encoder_->output_plane.getFormat(output_format);
  if (ret == 0) {
    output_y_stride_ = output_format.fmt.pix_mp.plane_fmt[0].bytesperline;
    if (output_is_nv12_) {
      output_u_stride_ = output_format.fmt.pix_mp.plane_fmt[1].bytesperline;
      output_v_stride_ = output_u_stride_;
    } else {
      output_u_stride_ = output_format.fmt.pix_mp.plane_fmt[1].bytesperline;
      output_v_stride_ = output_format.fmt.pix_mp.plane_fmt[2].bytesperline;
    }
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] getFormat: num_planes=%d, y_stride=%d, "
                   "u_stride=%d, v_stride=%d\n",
                   output_format.fmt.pix_mp.num_planes, output_y_stride_,
                   output_u_stride_, output_v_stride_);
      std::fflush(stderr);
    }
  } else if (verbose) {
    std::fprintf(stderr, "[MMAPI] getFormat failed: ret=%d\n", ret);
    std::fflush(stderr);
  }

  if (output_y_stride_ == 0) {
    output_y_stride_ = width_;
  }
  if (output_u_stride_ == 0) {
    // For NV12, the chroma plane has full-width interleaved UV.
    output_u_stride_ = output_is_nv12_ ? width_ : width_ / 2;
  }
  if (output_v_stride_ == 0) {
    // For NV12, V is interleaved with U in plane[1]; keep v_stride equal to
    // u_stride for logging only.
    output_v_stride_ = output_is_nv12_ ? output_u_stride_ : width_ / 2;
  }

  // Some Jetson drivers report incomplete/zero plane info via getFormat(). Clamp
  // to sane minimums to avoid under-striding NV12 (which can lead to empty
  // output or corruption).
  if (output_is_nv12_ && output_u_stride_ < width_) {
    output_u_stride_ = width_;
    output_v_stride_ = width_;
  }
  if (!output_is_nv12_) {
    const int min_chroma_stride = (width_ + 1) / 2;
    if (output_u_stride_ < min_chroma_stride) {
      output_u_stride_ = min_chroma_stride;
    }
    if (output_v_stride_ < min_chroma_stride) {
      output_v_stride_ = min_chroma_stride;
    }
  }

  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] Final strides: y=%d, u=%d, v=%d (is_nv12=%d)\n",
                 output_y_stride_, output_u_stride_, output_v_stride_,
                 output_is_nv12_ ? 1 : 0);
    std::fflush(stderr);
  }

  return true;
}

bool JetsonMmapiEncoder::SetupPlanes() {
  output_buffer_count_ = kDefaultOutputBufferCount;
  capture_buffer_count_ = kDefaultCaptureBufferCount;

  if (encoder_->output_plane.setupPlane(V4L2_MEMORY_MMAP,
                                        output_buffer_count_, true, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup output plane.";
    return false;
  }
  if (encoder_->capture_plane.setupPlane(V4L2_MEMORY_MMAP,
                                         capture_buffer_count_, true, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup capture plane.";
    return false;
  }
  return true;
}

bool JetsonMmapiEncoder::QueueCaptureBuffers() {
  for (int i = 0; i < capture_buffer_count_; ++i) {
    v4l2_buffer v4l2_buf = {};
    v4l2_plane planes[VIDEO_MAX_PLANES] = {};
    v4l2_buf.index = i;
    v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    v4l2_buf.memory = V4L2_MEMORY_MMAP;
    v4l2_buf.m.planes = planes;
    v4l2_buf.length = encoder_->capture_plane.getNumPlanes();
    if (encoder_->capture_plane.qBuffer(v4l2_buf, nullptr) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to queue capture buffer " << i;
      return false;
    }
  }
  return true;
}

bool JetsonMmapiEncoder::StartStreaming() {
  if (streaming_) {
    return true;
  }
  if (encoder_->output_plane.setStreamStatus(true) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start output plane stream.";
    return false;
  }
  if (encoder_->capture_plane.setStreamStatus(true) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start capture plane stream.";
    return false;
  }
  streaming_ = true;
  return true;
}

void JetsonMmapiEncoder::StopStreaming() {
  if (!streaming_ || !encoder_) {
    return;
  }
  encoder_->output_plane.setStreamStatus(false);
  encoder_->capture_plane.setStreamStatus(false);
  streaming_ = false;
}

bool JetsonMmapiEncoder::QueueOutputBuffer(const uint8_t* src_y,
                                           int stride_y,
                                           const uint8_t* src_u,
                                           int stride_u,
                                           const uint8_t* src_v,
                                           int stride_v) {
  static std::atomic<bool> logged_first_queue(false);
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;

  NvBuffer* buffer = encoder_->output_plane.getNthBuffer(next_output_index_);
  if (!buffer) {
    RTC_LOG(LS_ERROR) << "Failed to get output buffer.";
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBuffer: getNthBuffer(%d) returned null\n",
                 next_output_index_);
    std::fflush(stderr);
    return false;
  }

  if (!logged_first_queue.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBuffer: buffer=%p, n_planes=%d, "
                 "plane[0].data=%p, plane[0].fmt.bytesperpixel=%d\n",
                 static_cast<void*>(buffer), buffer->n_planes,
                 buffer->planes[0].data, buffer->planes[0].fmt.bytesperpixel);
    std::fflush(stderr);
  }

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  CopyPlane(dst_y, output_y_stride_, src_y, stride_y, width_, height_);
  if (output_is_nv12_) {
    uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
    const int chroma_width = width_ / 2;
    const int chroma_height = height_ / 2;
    for (int y = 0; y < chroma_height; ++y) {
      const uint8_t* src_u_row = src_u + y * stride_u;
      const uint8_t* src_v_row = src_v + y * stride_v;
      uint8_t* dst_row = dst_uv + y * output_u_stride_;
      for (int x = 0; x < chroma_width; ++x) {
        dst_row[x * 2] = src_u_row[x];
        dst_row[x * 2 + 1] = src_v_row[x];
      }
    }
  } else {
    uint8_t* dst_u = static_cast<uint8_t*>(buffer->planes[1].data);
    uint8_t* dst_v = static_cast<uint8_t*>(buffer->planes[2].data);
    CopyPlane(dst_u, output_u_stride_, src_u, stride_u, width_ / 2,
              height_ / 2);
    CopyPlane(dst_v, output_v_stride_, src_v, stride_v, width_ / 2,
              height_ / 2);
  }

  // IMPORTANT: In MMAP mode, the Jetson MMAPI wrapper can rely on NvBuffer's
  // bytesused values (not only the v4l2_buffer's plane bytesused). If these
  // are left at 0, the encoder may treat the input as empty and output black.
  buffer->planes[0].bytesused = output_y_stride_ * height_;
  buffer->planes[1].bytesused = output_u_stride_ * (height_ / 2);
  if (!output_is_nv12_ && buffer->n_planes > 2) {
    buffer->planes[2].bytesused = output_v_stride_ * (height_ / 2);
  }
  if (verbose) {
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      if (buffer->planes[plane].bytesused == 0 ||
          buffer->planes[plane].bytesused > buffer->planes[plane].length) {
        std::fprintf(stderr,
                     "[MMAPI] WARNING: output plane bytesused invalid: "
                     "plane=%d bytesused=%u length=%u\n",
                     plane, buffer->planes[plane].bytesused,
                     buffer->planes[plane].length);
        std::fflush(stderr);
      }
    }
  }

  for (int plane = 0; plane < buffer->n_planes; ++plane) {
    NvBufSurface* surface = nullptr;
    int map_ret =
        NvBufSurfaceFromFd(buffer->planes[plane].fd,
                           reinterpret_cast<void**>(&surface));
    if (map_ret != 0 || !surface) {
      RTC_LOG(LS_ERROR) << "Failed to map output plane for device sync.";
      std::fprintf(stderr,
                   "[MMAPI] NvBufSurfaceFromFd failed: plane=%d, fd=%d, "
                   "ret=%d, surface=%p\n",
                   plane, buffer->planes[plane].fd, map_ret,
                   static_cast<void*>(surface));
      std::fflush(stderr);
      return false;
    }
    int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, plane);
    if (sync_ret != 0) {
      RTC_LOG(LS_ERROR) << "Failed to sync output plane for device.";
      std::fprintf(stderr,
                   "[MMAPI] NvBufSurfaceSyncForDevice failed: plane=%d, "
                   "ret=%d\n",
                   plane, sync_ret);
      std::fflush(stderr);
      return false;
    }
  }

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  // Use the configured plane strides to satisfy driver expectations.
  planes[0].bytesused = output_y_stride_ * height_;
  planes[1].bytesused = output_u_stride_ * (height_ / 2);
  if (!output_is_nv12_) {
    planes[2].bytesused = output_v_stride_ * (height_ / 2);
  }

  int qbuf_ret = encoder_->output_plane.qBuffer(v4l2_buf, nullptr);
  if (qbuf_ret < 0) {
    RTC_LOG(LS_ERROR) << "Failed to queue output buffer.";
    std::fprintf(stderr,
                 "[MMAPI] output_plane.qBuffer failed: index=%d, ret=%d, "
                 "errno=%d (%s)\n",
                 next_output_index_, qbuf_ret, errno, strerror(errno));
    std::fflush(stderr);
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
  return true;
}

bool JetsonMmapiEncoder::QueueOutputBufferNV12(const uint8_t* src_y,
                                               int stride_y,
                                               const uint8_t* src_uv,
                                               int stride_uv) {
  static std::atomic<bool> logged_first_queue(false);
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;

  if (!output_is_nv12_) {
    RTC_LOG(LS_ERROR) << "QueueOutputBufferNV12 called but output is not NV12.";
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBufferNV12: output_is_nv12_=false\n");
      std::fflush(stderr);
    }
    return false;
  }

  NvBuffer* buffer = encoder_->output_plane.getNthBuffer(next_output_index_);
  if (!buffer) {
    RTC_LOG(LS_ERROR) << "Failed to get output buffer.";
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBufferNV12: getNthBuffer(%d) returned "
                 "null\n",
                 next_output_index_);
    std::fflush(stderr);
    return false;
  }

  if (!logged_first_queue.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBufferNV12: buffer=%p, n_planes=%d, "
                 "plane[0].data=%p, plane[0].fmt.bytesperpixel=%d\n",
                 static_cast<void*>(buffer), buffer->n_planes,
                 buffer->planes[0].data, buffer->planes[0].fmt.bytesperpixel);
    std::fflush(stderr);
  }

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
  CopyPlane(dst_y, output_y_stride_, src_y, stride_y, width_, height_);
  CopyPlane(dst_uv, output_u_stride_, src_uv, stride_uv, width_,
            height_ / 2);

  // Keep NvBuffer bytesused in sync for MMAP.
  buffer->planes[0].bytesused = output_y_stride_ * height_;
  buffer->planes[1].bytesused = output_u_stride_ * (height_ / 2);
  if (verbose) {
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      if (buffer->planes[plane].bytesused == 0 ||
          buffer->planes[plane].bytesused > buffer->planes[plane].length) {
        std::fprintf(stderr,
                     "[MMAPI] WARNING: output plane bytesused invalid: "
                     "plane=%d bytesused=%u length=%u\n",
                     plane, buffer->planes[plane].bytesused,
                     buffer->planes[plane].length);
        std::fflush(stderr);
      }
    }
  }

  for (int plane = 0; plane < buffer->n_planes; ++plane) {
    NvBufSurface* surface = nullptr;
    int map_ret =
        NvBufSurfaceFromFd(buffer->planes[plane].fd,
                           reinterpret_cast<void**>(&surface));
    if (map_ret != 0 || !surface) {
      RTC_LOG(LS_ERROR) << "Failed to map output plane for device sync.";
      std::fprintf(stderr,
                   "[MMAPI] NvBufSurfaceFromFd failed: plane=%d, fd=%d, "
                   "ret=%d, surface=%p\n",
                   plane, buffer->planes[plane].fd, map_ret,
                   static_cast<void*>(surface));
      std::fflush(stderr);
      return false;
    }
    int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, plane);
    if (sync_ret != 0) {
      RTC_LOG(LS_ERROR) << "Failed to sync output plane for device.";
      std::fprintf(stderr,
                   "[MMAPI] NvBufSurfaceSyncForDevice failed: plane=%d, "
                   "ret=%d\n",
                   plane, sync_ret);
      std::fflush(stderr);
      return false;
    }
  }

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  planes[0].bytesused = output_y_stride_ * height_;
  planes[1].bytesused = output_u_stride_ * (height_ / 2);

  int qbuf_ret = encoder_->output_plane.qBuffer(v4l2_buf, nullptr);
  if (qbuf_ret < 0) {
    RTC_LOG(LS_ERROR) << "Failed to queue output buffer.";
    std::fprintf(stderr,
                 "[MMAPI] output_plane.qBuffer failed: index=%d, ret=%d, "
                 "errno=%d (%s)\n",
                 next_output_index_, qbuf_ret, errno, strerror(errno));
    std::fflush(stderr);
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
  return true;
}

bool JetsonMmapiEncoder::DequeueCaptureBuffer(std::vector<uint8_t>* encoded,
                                              bool* is_keyframe) {
  static std::atomic<bool> dumped(false);
  static std::atomic<bool> logged_env(false);
  static std::atomic<int> verbose_left(10);
  static std::atomic<uint64_t> empty_frame_count(0);
  static std::atomic<uint64_t> timeout_count(0);
  static std::atomic<uint64_t> total_dequeue_count(0);
  const bool verbose = std::getenv("LK_DUMP_H264_VERBOSE") != nullptr;
  const bool debug = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  constexpr int kMaxEmptyRetries = 5;
  constexpr int kDequeueTimeoutMs = 1000;
  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  NvBuffer* buffer = nullptr;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->capture_plane.getNumPlanes();

  const uint64_t dequeue_num = total_dequeue_count.fetch_add(1);
  size_t bytesused = 0;
  int empty_retries = 0;
  for (int attempt = 0; attempt < kMaxEmptyRetries; ++attempt) {
    int dq_ret = encoder_->capture_plane.dqBuffer(v4l2_buf, &buffer, nullptr,
                                                  kDequeueTimeoutMs);
    if (dq_ret < 0) {
      timeout_count.fetch_add(1);
      RTC_LOG(LS_ERROR) << "Failed to dequeue capture buffer.";
      std::fprintf(stderr,
                   "[MMAPI] capture_plane.dqBuffer failed: ret=%d, errno=%d "
                   "(%s), timeout_count=%lu, dequeue_num=%lu\n",
                   dq_ret, errno, strerror(errno), timeout_count.load(),
                   dequeue_num);
      std::fflush(stderr);
      return false;
    }
    bytesused = v4l2_buf.m.planes[0].bytesused;
    if (bytesused > 0) {
      break;
    }
    empty_retries++;
    empty_frame_count.fetch_add(1);
    if (debug || dequeue_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] Empty capture buffer (attempt %d/%d, "
                   "total_empty=%lu)\n",
                   attempt + 1, kMaxEmptyRetries, empty_frame_count.load());
      std::fflush(stderr);
    }
    if (encoder_->capture_plane.qBuffer(v4l2_buf, nullptr) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to requeue empty capture buffer.";
      std::fprintf(stderr, "[MMAPI] Failed to requeue empty capture buffer\n");
      std::fflush(stderr);
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }

  if (bytesused == 0) {
    std::fprintf(stderr,
                 "[MMAPI] WARNING: All %d dequeue attempts returned empty "
                 "buffer (dequeue_num=%lu)\n",
                 kMaxEmptyRetries, dequeue_num);
    std::fflush(stderr);
  }

  encoded->assign(static_cast<uint8_t*>(buffer->planes[0].data),
                  static_cast<uint8_t*>(buffer->planes[0].data) + bytesused);
  if (is_keyframe) {
    *is_keyframe = (v4l2_buf.flags & V4L2_BUF_FLAG_KEYFRAME) != 0;
  }
  if ((verbose || debug) && verbose_left.load(std::memory_order_relaxed) > 0) {
    const int remaining = verbose_left.fetch_sub(1);
    if (remaining > 0) {
      std::fprintf(stderr,
                   "[MMAPI] capture dqBuffer: bytesused=%zu flags=0x%x "
                   "index=%d empty_retries=%d\n",
                   bytesused, v4l2_buf.flags, v4l2_buf.index, empty_retries);
      std::fflush(stderr);
    }
  }

  if (!dumped.load(std::memory_order_relaxed)) {
    const char* dump_path = std::getenv("LK_DUMP_H264");
    if (!dump_path || dump_path[0] == '\0') {
      if (!logged_env.exchange(true)) {
        std::fprintf(stderr,
                     "LK_DUMP_H264 not set; skipping H264 dump (MMAPI).\n");
        std::fflush(stderr);
      }
    } else if (bytesused == 0) {
      if (!logged_env.exchange(true)) {
        std::error_code ec;
        std::filesystem::path path(dump_path);
        if (path.has_parent_path()) {
          std::filesystem::create_directories(path.parent_path(), ec);
        }

        // Create/truncate the file so it's obvious the env var was applied,
        // even if the first dequeued buffers are empty.
        std::ofstream out(dump_path, std::ios::binary);
        if (out.good()) {
          std::fprintf(
              stderr,
              "LK_DUMP_H264 set to %s but packet is empty (MMAPI); created "
              "empty dump file\n",
              dump_path);
          std::fflush(stderr);
        } else {
          std::fprintf(stderr,
                       "Failed to open LK_DUMP_H264 path (MMAPI): %s\n",
                       dump_path);
          std::fflush(stderr);
        }
      }
    } else {
      std::error_code ec;
      std::filesystem::path path(dump_path);
      if (path.has_parent_path()) {
        std::filesystem::create_directories(path.parent_path(), ec);
      }
      std::ofstream out(dump_path, std::ios::binary);
      if (out.good()) {
        out.write(reinterpret_cast<const char*>(encoded->data()),
                  static_cast<std::streamsize>(encoded->size()));
        std::fprintf(stderr,
                     "Dumped H264 access unit to %s (MMAPI, bytes=%zu, "
                     "keyframe=%d)\n",
                     dump_path, encoded->size(),
                     is_keyframe ? (*is_keyframe ? 1 : 0) : -1);
        std::fflush(stderr);
        dumped.store(true, std::memory_order_relaxed);
      } else {
        std::fprintf(stderr,
                     "Failed to open LK_DUMP_H264 path (MMAPI): %s\n",
                     dump_path);
        std::fflush(stderr);
      }
      logged_env.store(true, std::memory_order_relaxed);
    }
  }

  int requeue_ret = encoder_->capture_plane.qBuffer(v4l2_buf, nullptr);
  if (requeue_ret < 0) {
    RTC_LOG(LS_ERROR) << "Failed to requeue capture buffer.";
    std::fprintf(stderr,
                 "[MMAPI] Failed to requeue capture buffer: ret=%d, errno=%d "
                 "(%s)\n",
                 requeue_ret, errno, strerror(errno));
    std::fflush(stderr);
    return false;
  }
  return true;
}

bool JetsonMmapiEncoder::DequeueOutputBuffer() {
  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  if (encoder_->output_plane.dqBuffer(v4l2_buf, nullptr, nullptr, 0) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to dequeue output buffer.";
    return false;
  }
  return true;
}

bool JetsonMmapiEncoder::ForceKeyframe() {
  v4l2_ext_control control = {};
  v4l2_ext_controls controls = {};
  control.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
  control.value = 1;
  controls.count = 1;
  controls.controls = &control;
  return encoder_->setExtControls(controls) == 0;
}

}  // namespace livekit
