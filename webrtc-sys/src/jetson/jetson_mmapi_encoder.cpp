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

// Try to read the true per-plane pitch/height for an NvBuffer plane FD.
// This avoids relying on NvBufferGetParams/nvbuf_utils.h (not present on all
// JetPack images), and is more reliable than NvBufferPlane::fmt fields in MMAP
// mode.
bool GetPitchAndHeightFromNvBufSurfaceFd(int dmabuf_fd,
                                        int plane_index,
                                        uint32_t* out_pitch,
                                        uint32_t* out_height,
                                        uint32_t* out_num_planes) {
  if (out_pitch) *out_pitch = 0;
  if (out_height) *out_height = 0;
  if (out_num_planes) *out_num_planes = 0;
  if (dmabuf_fd < 0) {
    return false;
  }
  if (plane_index < 0) {
    return false;
  }

  NvBufSurface* surface = nullptr;
  int ret = NvBufSurfaceFromFd(dmabuf_fd, reinterpret_cast<void**>(&surface));
  if (ret != 0 || !surface) {
    return false;
  }
  // Most MMAPI buffers are not batched; guard anyway.
  if (surface->batchSize < 1) {
    return false;
  }

  const NvBufSurfaceParams& p = surface->surfaceList[0];
  if (out_num_planes) {
    *out_num_planes = p.planeParams.num_planes;
  }
  if (p.planeParams.num_planes < 1) {
    return false;
  }
  if (plane_index >= static_cast<int>(p.planeParams.num_planes)) {
    return false;
  }

  const uint32_t pitch = p.planeParams.pitch[plane_index];
  const uint32_t height = p.planeParams.height[plane_index];
  if (out_pitch) *out_pitch = pitch;
  if (out_height) *out_height = height;
  return pitch > 0 && height > 0;
}

#ifndef V4L2_PIX_FMT_H265
#define V4L2_PIX_FMT_H265 v4l2_fourcc('H', '2', '6', '5')
#endif

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
    const uint32_t fallback_pixfmt = CodecToV4L2FallbackPixFmt(codec);
    if (fallback_pixfmt == pixfmt ||
        encoder->setCapturePlaneFormat(fallback_pixfmt, 64, 64,
                                       kMinBitstreamBufferSize) < 0) {
      return false;
    }
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

uint32_t JetsonMmapiEncoder::CodecToV4L2FallbackPixFmt(JetsonCodec codec) {
  if (codec == JetsonCodec::kH265) {
    return V4L2_PIX_FMT_H265;
  }
  return CodecToV4L2PixFmt(codec);
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
    const uint32_t fallback_pixfmt = CodecToV4L2FallbackPixFmt(codec_);
    int ret_fallback = ret;
    if (fallback_pixfmt != codec_pixfmt) {
      ret_fallback = encoder_->setCapturePlaneFormat(
          fallback_pixfmt, width_, height_, bitstream_size);
    }
    if (fallback_pixfmt == codec_pixfmt || ret_fallback < 0) {
      RTC_LOG(LS_ERROR) << "Failed to set capture plane format.";
      std::fprintf(stderr,
                   "[MMAPI] setCapturePlaneFormat failed: ret=%d, errno=%d "
                   "(%s)\n",
                   ret_fallback, errno, strerror(errno));
      std::fflush(stderr);
      return false;
    }
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] setCapturePlaneFormat fallback succeeded (pixfmt=0x%x)\n",
                   fallback_pixfmt);
      std::fflush(stderr);
    }
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
  // Some MMAPI wrappers/driver paths require v4l2_format.type to be set before
  // querying the current format, otherwise the returned struct can be zeroed.
  output_format.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
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
  static std::atomic<bool> logged_plane_layout(false);
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
  if (output_is_nv12_) {
    if (buffer->n_planes < 2) {
      RTC_LOG(LS_ERROR) << "Output plane format is NV12 but has <2 planes.";
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBuffer: NV12 requires 2 planes, got %d\n",
                   buffer->n_planes);
      std::fflush(stderr);
      return false;
    }
  } else {
    if (buffer->n_planes < 3) {
      RTC_LOG(LS_ERROR) << "Output plane format is YUV420M but has <3 planes.";
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBuffer: YUV420M requires 3 planes, got %d\n",
                   buffer->n_planes);
      std::fflush(stderr);
      return false;
    }
  }

  if (!logged_first_queue.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBuffer: buffer=%p, n_planes=%d, "
                 "plane[0].data=%p, plane[0].fmt.bytesperpixel=%d, "
                 "plane[0].fmt.stride=%d, plane[0].length=%u\n",
                 static_cast<void*>(buffer), buffer->n_planes,
                 buffer->planes[0].data, buffer->planes[0].fmt.bytesperpixel,
                 static_cast<int>(buffer->planes[0].fmt.stride),
                 buffer->planes[0].length);
    std::fflush(stderr);
  }

  // IMPORTANT: The actual mapped destination pitch can differ from the
  // V4L2 bytesperline returned by getFormat() due to alignment/pitch
  // requirements. Using the wrong destination stride will produce
  // "striped/shifted/green" output.
  //
  // On some Jetson/MMAPI combinations, NvBufferPlane::fmt.stride can be unset
  // in MMAP mode. Also, NvBufferPlane::fmt.height may be misleading (e.g. UV
  // plane reporting luma height), which can cause derived stride to be too
  // small and result in green/corrupted output.
  //
  // Prefer NvBufSurfaceFromFd() plane pitch/height when available, and clamp
  // derived strides to never under/over-stride the plane.
  const int chroma_height = (height_ + 1) / 2;
  const int chroma_width = (width_ + 1) / 2;
  uint32_t y_pitch = 0, y_h = 0, y_np = 0;
  uint32_t u_pitch = 0, u_h = 0, u_np = 0;
  uint32_t v_pitch = 0, v_h = 0, v_np = 0;
  const bool have_y =
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[0].fd, 0, &y_pitch, &y_h, &y_np);
  const bool have_u =
      (buffer->n_planes > 1) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[1].fd, 1, &u_pitch, &u_h, &u_np);
  const bool have_v =
      (!output_is_nv12_ && buffer->n_planes > 2) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[2].fd, 2, &v_pitch, &v_h, &v_np);

  auto stride_from_plane = [&](int plane_index,
                               const NvBuffer::NvBufferPlane& plane,
                               int plane_height,
                               int min_stride,
                               int fallback) -> int {
    // 1) NvBufferPlane::fmt.stride if present and sane.
    const int fmt_stride = static_cast<int>(plane.fmt.stride);
    if (fmt_stride >= min_stride) {
      return fmt_stride;
    }

    // 2) NvBufSurfaceFromFd pitch (ground truth for this plane fd).
    if (plane_index == 0 && have_y) {
      const int pitch = static_cast<int>(y_pitch);
      if (pitch >= min_stride) return pitch;
    } else if (plane_index == 1 && have_u) {
      const int pitch = static_cast<int>(u_pitch);
      if (pitch >= min_stride) return pitch;
    } else if (plane_index == 2 && have_v) {
      const int pitch = static_cast<int>(v_pitch);
      if (pitch >= min_stride) return pitch;
    }

    // 3) Derive from mapped plane length. Be careful: plane.fmt.height can be
    // misleading (e.g. UV plane reporting full luma height), which would yield
    // an under-stride (pitch/2) and a green image. Only accept a derived value
    // if it meets min_stride.
    int best = 0;
    if (plane.length > 0) {
      const int fmt_h = static_cast<int>(plane.fmt.height);
      if (fmt_h > 0) {
        const int derived =
            static_cast<int>(plane.length / static_cast<uint32_t>(fmt_h));
        if (derived >= min_stride) {
          best = derived;
        }
      }
      if (best == 0 && plane_height > 0) {
        const int derived = static_cast<int>(plane.length /
                                             static_cast<uint32_t>(plane_height));
        if (derived >= min_stride) {
          best = derived;
        }
      }
    }

    int stride = best > 0 ? best : fallback;
    if (stride < min_stride) {
      stride = min_stride;
    }

    // Cap stride to what the allocation can accommodate to avoid walking past
    // the mapped plane. (This should not normally trigger.)
    if (plane_height > 0 && plane.length > 0) {
      const int max_stride = static_cast<int>(
          plane.length / static_cast<uint32_t>(plane_height));
      if (max_stride > 0 && stride > max_stride) {
        stride = max_stride;
      }
    }
    return stride;
  };

  const int min_y_stride = width_;
  const int min_uv_stride = output_is_nv12_ ? width_ : chroma_width;
  const int dst_y_stride =
      stride_from_plane(0, buffer->planes[0], height_, min_y_stride, output_y_stride_);
  const int dst_u_stride =
      stride_from_plane(1, buffer->planes[1], chroma_height, min_uv_stride, output_u_stride_);
  const int dst_v_stride =
      (!output_is_nv12_ && buffer->n_planes > 2)
          ? stride_from_plane(2, buffer->planes[2], chroma_height, chroma_width,
                              output_v_stride_)
          : dst_u_stride;

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  const int plane_y_height =
      (have_y && static_cast<int>(y_h) >= height_) ? static_cast<int>(y_h)
                                                   : height_;
  const int plane_u_height =
      (have_u && static_cast<int>(u_h) >= chroma_height) ? static_cast<int>(u_h)
                                                         : chroma_height;
  const int plane_v_height =
      (have_v && static_cast<int>(v_h) >= chroma_height) ? static_cast<int>(v_h)
                                                         : chroma_height;
  auto ZeroPlaneRows = [](uint8_t* dst, int stride, int start_row,
                          int end_row) {
    for (int y = start_row; y < end_row; ++y) {
      std::memset(dst + y * stride, 0, static_cast<size_t>(stride));
    }
  };
  CopyPlane(dst_y, dst_y_stride, src_y, stride_y, width_, height_);
  if (plane_y_height > height_) {
    ZeroPlaneRows(dst_y, dst_y_stride, height_, plane_y_height);
  }
  if (output_is_nv12_) {
    uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
    for (int y = 0; y < chroma_height; ++y) {
      const uint8_t* src_u_row = src_u + y * stride_u;
      const uint8_t* src_v_row = src_v + y * stride_v;
      uint8_t* dst_row = dst_uv + y * dst_u_stride;
      for (int x = 0; x < chroma_width; ++x) {
        dst_row[x * 2] = src_u_row[x];
        dst_row[x * 2 + 1] = src_v_row[x];
      }
    }
    if (plane_u_height > chroma_height) {
      ZeroPlaneRows(dst_uv, dst_u_stride, chroma_height, plane_u_height);
    }
  } else {
    uint8_t* dst_u = static_cast<uint8_t*>(buffer->planes[1].data);
    uint8_t* dst_v = static_cast<uint8_t*>(buffer->planes[2].data);
    CopyPlane(dst_u, dst_u_stride, src_u, stride_u, chroma_width,
              chroma_height);
    CopyPlane(dst_v, dst_v_stride, src_v, stride_v, chroma_width,
              chroma_height);
    if (plane_u_height > chroma_height) {
      ZeroPlaneRows(dst_u, dst_u_stride, chroma_height, plane_u_height);
    }
    if (plane_v_height > chroma_height) {
      ZeroPlaneRows(dst_v, dst_v_stride, chroma_height, plane_v_height);
    }
  }

  // IMPORTANT: In MMAP mode, the Jetson MMAPI wrapper can rely on NvBuffer's
  // bytesused values (not only the v4l2_buffer's plane bytesused). If these
  // are left at 0, the encoder may treat the input as empty and output black.
  buffer->planes[0].bytesused =
      dst_y_stride * static_cast<uint32_t>(plane_y_height);
  buffer->planes[1].bytesused =
      dst_u_stride * static_cast<uint32_t>(plane_u_height);
  if (!output_is_nv12_ && buffer->n_planes > 2) {
    buffer->planes[2].bytesused =
        dst_v_stride * static_cast<uint32_t>(plane_v_height);
  }

  if (verbose && !logged_plane_layout.exchange(true)) {
    // One-time "ground truth" layout print: what we *detected* and what we
    // *tell the driver* via bytesused.
    std::fprintf(
        stderr,
        "[MMAPI] Output plane layout: w=%d h=%d is_nv12=%d | "
        "dst_strides(y,u,v)=(%d,%d,%d) | "
        "plane_heights(y,u,v)=(%d,%d,%d) | "
        "bytesused(y,u,v)=(%u,%u,%u)\n",
        width_, height_, output_is_nv12_ ? 1 : 0, dst_y_stride, dst_u_stride,
        dst_v_stride, plane_y_height, plane_u_height, plane_v_height,
        buffer->planes[0].bytesused, buffer->planes[1].bytesused,
        (!output_is_nv12_ && buffer->n_planes > 2) ? buffer->planes[2].bytesused
                                                   : 0u);
    if (buffer->n_planes > 0) {
      std::fprintf(stderr,
                   "[MMAPI] plane[0]: fmt.stride=%d fmt.height=%d length=%u\n",
                   static_cast<int>(buffer->planes[0].fmt.stride),
                   static_cast<int>(buffer->planes[0].fmt.height),
                   buffer->planes[0].length);
    }
    if (buffer->n_planes > 1) {
      std::fprintf(stderr,
                   "[MMAPI] plane[1]: fmt.stride=%d fmt.height=%d length=%u\n",
                   static_cast<int>(buffer->planes[1].fmt.stride),
                   static_cast<int>(buffer->planes[1].fmt.height),
                   buffer->planes[1].length);
    }
    if (!output_is_nv12_ && buffer->n_planes > 2) {
      std::fprintf(stderr,
                   "[MMAPI] plane[2]: fmt.stride=%d fmt.height=%d length=%u\n",
                   static_cast<int>(buffer->planes[2].fmt.stride),
                   static_cast<int>(buffer->planes[2].fmt.height),
                   buffer->planes[2].length);
    }
    std::fprintf(stderr,
                 "[MMAPI] NvBufSurfaceFromFd pitch/height (per-plane fd): "
                 "Y(ok=%d pitch=%u h=%u np=%u) "
                 "U(ok=%d pitch=%u h=%u np=%u) "
                 "V(ok=%d pitch=%u h=%u np=%u)\n",
                 have_y ? 1 : 0, y_pitch, y_h, y_np,
                 have_u ? 1 : 0, u_pitch, u_h, u_np,
                 have_v ? 1 : 0, v_pitch, v_h, v_np);
    std::fflush(stderr);
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

  // Sync CPU-written pixel data to the device.  Each V4L2 MMAP plane has its
  // own DMA fd.  NvBufSurfaceFromFd returns a surface that represents the
  // *entire* multi-plane allocation, so the plane parameter to
  // NvBufSurfaceSyncForDevice must be the NvBufSurface plane index.  For
  // multi-planar V4L2 formats where all planes share the same fd, we sync
  // all planes at once (-1).  When each plane has a distinct fd, we sync
  // plane 0 of each surface (the only plane that fd covers).
  {
    // Collect unique fds to avoid double-syncing when planes share a fd.
    int synced_fds[VIDEO_MAX_PLANES] = {-1, -1, -1, -1};
    int n_synced = 0;
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      int fd = buffer->planes[plane].fd;
      bool already = false;
      for (int j = 0; j < n_synced; ++j) {
        if (synced_fds[j] == fd) { already = true; break; }
      }
      if (already) continue;
      synced_fds[n_synced++] = fd;

      NvBufSurface* surface = nullptr;
      int map_ret =
          NvBufSurfaceFromFd(fd, reinterpret_cast<void**>(&surface));
      if (map_ret != 0 || !surface) {
        RTC_LOG(LS_ERROR) << "Failed to map output plane for device sync.";
        std::fprintf(stderr,
                     "[MMAPI] NvBufSurfaceFromFd failed: plane=%d, fd=%d, "
                     "ret=%d, surface=%p\n",
                     plane, fd, map_ret,
                     static_cast<void*>(surface));
        std::fflush(stderr);
        return false;
      }
      // Sync all planes of this surface at once.
      int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, -1);
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
  }

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  planes[0].bytesused = buffer->planes[0].bytesused;
  planes[1].bytesused = buffer->planes[1].bytesused;
  if (!output_is_nv12_) {
    planes[2].bytesused = buffer->planes[2].bytesused;
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
  static std::atomic<bool> logged_plane_layout(false);
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
                 "plane[0].data=%p, plane[0].fmt.bytesperpixel=%d, "
                 "plane[0].fmt.stride=%d, plane[0].length=%u\n",
                 static_cast<void*>(buffer), buffer->n_planes,
                 buffer->planes[0].data, buffer->planes[0].fmt.bytesperpixel,
                 static_cast<int>(buffer->planes[0].fmt.stride),
                 buffer->planes[0].length);
    std::fflush(stderr);
  }

  const int chroma_height = (height_ + 1) / 2;
  uint32_t y_pitch = 0, y_h = 0, y_np = 0;
  uint32_t uv_pitch = 0, uv_h = 0, uv_np = 0;
  const bool have_y =
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[0].fd, 0, &y_pitch, &y_h, &y_np);
  const bool have_uv =
      (buffer->n_planes > 1) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[1].fd, 1, &uv_pitch, &uv_h, &uv_np);

  auto stride_from_plane = [&](int plane_index,
                               const NvBuffer::NvBufferPlane& plane,
                               int plane_height,
                               int min_stride,
                               int fallback) -> int {
    const int fmt_stride = static_cast<int>(plane.fmt.stride);
    if (fmt_stride >= min_stride) {
      return fmt_stride;
    }
    if (plane_index == 0 && have_y) {
      const int pitch = static_cast<int>(y_pitch);
      if (pitch >= min_stride) return pitch;
    } else if (plane_index == 1 && have_uv) {
      const int pitch = static_cast<int>(uv_pitch);
      if (pitch >= min_stride) return pitch;
    }
    int best = 0;
    if (plane.length > 0) {
      const int fmt_h = static_cast<int>(plane.fmt.height);
      if (fmt_h > 0) {
        const int derived =
            static_cast<int>(plane.length / static_cast<uint32_t>(fmt_h));
        if (derived >= min_stride) {
          best = derived;
        }
      }
      if (best == 0 && plane_height > 0) {
        const int derived = static_cast<int>(plane.length /
                                             static_cast<uint32_t>(plane_height));
        if (derived >= min_stride) {
          best = derived;
        }
      }
    }

    int stride = best > 0 ? best : fallback;
    if (stride < min_stride) {
      stride = min_stride;
    }
    if (plane_height > 0 && plane.length > 0) {
      const int max_stride = static_cast<int>(
          plane.length / static_cast<uint32_t>(plane_height));
      if (max_stride > 0 && stride > max_stride) {
        stride = max_stride;
      }
    }
    return stride;
  };

  const int min_y_stride = width_;
  const int min_uv_stride = width_;
  const int dst_y_stride =
      stride_from_plane(0, buffer->planes[0], height_, min_y_stride, output_y_stride_);
  const int dst_uv_stride =
      stride_from_plane(1, buffer->planes[1], chroma_height, min_uv_stride, output_u_stride_);

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
  const int plane_y_height =
      (have_y && static_cast<int>(y_h) >= height_) ? static_cast<int>(y_h)
                                                   : height_;
  const int plane_uv_height =
      (have_uv && static_cast<int>(uv_h) >= chroma_height)
          ? static_cast<int>(uv_h)
          : chroma_height;
  auto ZeroPlaneRows = [](uint8_t* dst, int stride, int start_row,
                          int end_row) {
    for (int y = start_row; y < end_row; ++y) {
      std::memset(dst + y * stride, 0, static_cast<size_t>(stride));
    }
  };
  CopyPlane(dst_y, dst_y_stride, src_y, stride_y, width_, height_);
  CopyPlane(dst_uv, dst_uv_stride, src_uv, stride_uv, width_,
            (height_ + 1) / 2);
  if (plane_y_height > height_) {
    ZeroPlaneRows(dst_y, dst_y_stride, height_, plane_y_height);
  }
  if (plane_uv_height > chroma_height) {
    ZeroPlaneRows(dst_uv, dst_uv_stride, chroma_height, plane_uv_height);
  }

  // Keep NvBuffer bytesused in sync for MMAP.
  buffer->planes[0].bytesused =
      dst_y_stride * static_cast<uint32_t>(plane_y_height);
  buffer->planes[1].bytesused =
      dst_uv_stride * static_cast<uint32_t>(plane_uv_height);
  if (verbose && !logged_plane_layout.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] Output plane layout (NV12): w=%d h=%d | "
                 "dst_strides(y,uv)=(%d,%d) | plane_heights(y,uv)=(%d,%d) | "
                 "bytesused(y,uv)=(%u,%u)\n",
                 width_, height_, dst_y_stride, dst_uv_stride, plane_y_height,
                 plane_uv_height, buffer->planes[0].bytesused,
                 buffer->planes[1].bytesused);
    std::fprintf(stderr,
                 "[MMAPI] NvBufSurfaceFromFd pitch/height (per-plane fd): "
                 "Y(ok=%d pitch=%u h=%u np=%u) "
                 "UV(ok=%d pitch=%u h=%u np=%u)\n",
                 have_y ? 1 : 0, y_pitch, y_h, y_np,
                 have_uv ? 1 : 0, uv_pitch, uv_h, uv_np);
    std::fflush(stderr);
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

  // Sync CPU-written pixel data to the device (see QueueOutputBuffer for
  // detailed comments on the fd/plane mapping).
  {
    int synced_fds[VIDEO_MAX_PLANES] = {-1, -1, -1, -1};
    int n_synced = 0;
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      int fd = buffer->planes[plane].fd;
      bool already = false;
      for (int j = 0; j < n_synced; ++j) {
        if (synced_fds[j] == fd) { already = true; break; }
      }
      if (already) continue;
      synced_fds[n_synced++] = fd;

      NvBufSurface* surface = nullptr;
      int map_ret =
          NvBufSurfaceFromFd(fd, reinterpret_cast<void**>(&surface));
      if (map_ret != 0 || !surface) {
        RTC_LOG(LS_ERROR) << "Failed to map output plane for device sync.";
        std::fprintf(stderr,
                     "[MMAPI] NvBufSurfaceFromFd failed: plane=%d, fd=%d, "
                     "ret=%d, surface=%p\n",
                     plane, fd, map_ret,
                     static_cast<void*>(surface));
        std::fflush(stderr);
        return false;
      }
      int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, -1);
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
  }

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  planes[0].bytesused = buffer->planes[0].bytesused;
  planes[1].bytesused = buffer->planes[1].bytesused;

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

bool JetsonMmapiEncoder::EncodeDmaBuf(int dmabuf_fd,
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
                   "[MMAPI] EncodeDmaBuf() called but encoder not initialized "
                   "(initialized=%d, encoder=%p)\n",
                   initialized_ ? 1 : 0, static_cast<void*>(encoder_));
      std::fflush(stderr);
    }
    fail_count.fetch_add(1);
    return false;
  }

  // On first DmaBuf encode, re-setup the output plane for V4L2_MEMORY_DMABUF.
  if (!dmabuf_planes_setup_) {
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] EncodeDmaBuf: first call, setting up DMABUF planes\n");
      std::fflush(stderr);
    }
    // Stop streaming, reconfigure output plane, restart.
    StopStreaming();
    if (!SetupPlanesDmaBuf()) {
      std::fprintf(stderr,
                   "[MMAPI] EncodeDmaBuf: SetupPlanesDmaBuf() failed\n");
      std::fflush(stderr);
      fail_count.fetch_add(1);
      return false;
    }
    if (!QueueCaptureBuffers()) {
      std::fprintf(stderr,
                   "[MMAPI] EncodeDmaBuf: QueueCaptureBuffers() failed\n");
      std::fflush(stderr);
      fail_count.fetch_add(1);
      return false;
    }
    if (!StartStreaming()) {
      std::fprintf(stderr,
                   "[MMAPI] EncodeDmaBuf: StartStreaming() failed\n");
      std::fflush(stderr);
      fail_count.fetch_add(1);
      return false;
    }
    dmabuf_planes_setup_ = true;
    use_dmabuf_input_ = true;
    next_output_index_ = 0;
  }

  if (!logged_first_encode.exchange(true)) {
    std::fprintf(stderr,
                 "[MMAPI] First EncodeDmaBuf() call: dmabuf_fd=%d, "
                 "force_keyframe=%d\n",
                 dmabuf_fd, force_keyframe ? 1 : 0);
    std::fflush(stderr);
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
    if (verbose) {
      std::fprintf(stderr, "[MMAPI] ForceKeyframe() failed\n");
      std::fflush(stderr);
    }
  }

  if (!QueueOutputBufferDmaBuf(dmabuf_fd)) {
    if (verbose || frame_num < 10) {
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBufferDmaBuf() failed (frame %lu)\n",
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
                 "[MMAPI] EncodeDmaBuf() succeeded (frame %lu, encoded_size=%zu, "
                 "keyframe=%d, success=%lu, fail=%lu)\n",
                 frame_num, encoded->size(), is_keyframe ? *is_keyframe : -1,
                 success_count.load(), fail_count.load());
    std::fflush(stderr);
  }

  return true;
}

bool JetsonMmapiEncoder::SetupPlanesDmaBuf() {
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;

  // The DMA buffers from Argus are NV12.  If the encoder was initially
  // configured for YUV420M (3-plane I420), reconfigure to NV12M so the
  // plane count matches the DMA buffer layout.
  if (!output_is_nv12_) {
    int ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_, height_);
    if (ret < 0) {
      std::fprintf(stderr,
                   "[MMAPI] SetupPlanesDmaBuf: failed to switch output plane "
                   "to NV12M (ret=%d, errno=%d: %s)\n",
                   ret, errno, strerror(errno));
      std::fflush(stderr);
      return false;
    }
    output_is_nv12_ = true;
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] SetupPlanesDmaBuf: switched output plane to NV12M\n");
      std::fflush(stderr);
    }
  }

  // Output plane uses V4L2_MEMORY_DMABUF: we request buffers but don't
  // allocate backing memory -- the caller provides DMA fds at queue time.
  output_buffer_count_ = kDefaultOutputBufferCount;
  capture_buffer_count_ = kDefaultCaptureBufferCount;

  if (encoder_->output_plane.setupPlane(V4L2_MEMORY_DMABUF,
                                        output_buffer_count_, false, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup output plane for DMABUF.";
    if (verbose) {
      std::fprintf(stderr,
                   "[MMAPI] SetupPlanesDmaBuf: output_plane.setupPlane "
                   "V4L2_MEMORY_DMABUF failed, errno=%d (%s)\n",
                   errno, strerror(errno));
      std::fflush(stderr);
    }
    return false;
  }

  // Capture plane remains MMAP (encoded bitstream output).
  if (encoder_->capture_plane.setupPlane(V4L2_MEMORY_MMAP,
                                         capture_buffer_count_, true, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup capture plane.";
    return false;
  }

  if (verbose) {
    std::fprintf(stderr,
                 "[MMAPI] SetupPlanesDmaBuf: output=DMABUF(%d bufs), "
                 "capture=MMAP(%d bufs), format=NV12M\n",
                 output_buffer_count_, capture_buffer_count_);
    std::fflush(stderr);
  }

  return true;
}

bool JetsonMmapiEncoder::QueueOutputBufferDmaBuf(int dmabuf_fd) {
  static std::atomic<bool> logged_first(false);
  const bool verbose = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  const bool is_first = !logged_first.exchange(true);

  if (is_first) {
    std::fprintf(stderr,
                 "[MMAPI] QueueOutputBufferDmaBuf: fd=%d, index=%d\n",
                 dmabuf_fd, next_output_index_);
    std::fflush(stderr);
  }

  // Look up the NvBufSurface metadata for plane layout.
  NvBufSurface* surface = nullptr;
  int ret = NvBufSurfaceFromFd(dmabuf_fd, reinterpret_cast<void**>(&surface));
  if (ret != 0 || !surface) {
    RTC_LOG(LS_ERROR) << "QueueOutputBufferDmaBuf: NvBufSurfaceFromFd failed "
                      << "(fd=" << dmabuf_fd << ", ret=" << ret << ")";
    return false;
  }

  // The DMA buffer was filled by a GPU-side blit (Argus copyToNvBuffer).
  // The V4L2 encoder reads it via DMA, so a CPU cache sync is not required
  // and would fail with "Wrong buffer index" on some JetPack versions when
  // the surface was obtained via NvBufSurfaceFromFd rather than being the
  // original NvBufSurfaceCreate handle.
  //
  // If a sync *is* needed on a particular platform, the Argus shim should
  // perform it right after copyToNvBuffer while it still holds the original
  // surface pointer.

  // Determine plane count and bytesused from the NvBufSurface metadata.
  const NvBufSurfaceParams& params = surface->surfaceList[0];
  const uint32_t num_planes = params.planeParams.num_planes;

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_DMABUF;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();

  // For DMABUF mode, each v4l2 plane's m.fd is set to the DMA fd.
  // Multi-planar formats (YUV420M) share the same NvBufSurface fd but
  // different plane offsets; the driver resolves planes from the surface.
  for (uint32_t i = 0; i < v4l2_buf.length && i < num_planes; ++i) {
    planes[i].m.fd = dmabuf_fd;
    planes[i].bytesused =
        params.planeParams.pitch[i] * params.planeParams.height[i];
  }

  if (is_first || verbose) {
    for (uint32_t i = 0; i < v4l2_buf.length && i < num_planes; ++i) {
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBufferDmaBuf: plane[%u] fd=%d "
                   "pitch=%u height=%u bytesused=%u\n",
                   i, planes[i].m.fd,
                   params.planeParams.pitch[i],
                   params.planeParams.height[i],
                   planes[i].bytesused);
    }
    std::fflush(stderr);
  }

  int qbuf_ret = encoder_->output_plane.qBuffer(v4l2_buf, nullptr);
  if (qbuf_ret < 0) {
    RTC_LOG(LS_ERROR) << "QueueOutputBufferDmaBuf: qBuffer failed "
                      << "(index=" << next_output_index_
                      << ", errno=" << errno << ": " << strerror(errno) << ")";
    if (is_first || verbose) {
      std::fprintf(stderr,
                   "[MMAPI] QueueOutputBufferDmaBuf qBuffer failed: "
                   "index=%d, v4l2_buf.length=%u, num_planes=%u, "
                   "errno=%d (%s)\n",
                   next_output_index_, v4l2_buf.length, num_planes,
                   errno, strerror(errno));
      std::fflush(stderr);
    }
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
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
