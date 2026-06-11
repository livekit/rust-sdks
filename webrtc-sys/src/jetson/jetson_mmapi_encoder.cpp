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

#include "jetson_mmapi_encoder.h"

#include "jetson_plane_layout.h"

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
#include <unordered_map>

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

#ifndef V4L2_PIX_FMT_AV1
#define V4L2_PIX_FMT_AV1 v4l2_fourcc('A', 'V', '1', '0')
#endif

#ifndef V4L2_CID_MPEG_VIDEOENC_AV1_HEADERS_WITH_FRAME
#define V4L2_CID_MPEG_VIDEOENC_AV1_HEADERS_WITH_FRAME (V4L2_CID_MPEG_BASE + 569)
#endif

#ifndef V4L2_CID_MPEG_VIDEOENC_FORCE_INTRA_FRAME
#define V4L2_CID_MPEG_VIDEOENC_FORCE_INTRA_FRAME (V4L2_CID_MPEG_BASE + 566)
#endif

#ifndef V4L2_CID_MPEG_VIDEOENC_FORCE_IDR_FRAME
#define V4L2_CID_MPEG_VIDEOENC_FORCE_IDR_FRAME (V4L2_CID_MPEG_BASE + 567)
#endif

#ifndef V4L2_CID_MPEG_VIDEOENC_AV1_ENABLE_TILE_GROUPS
#define V4L2_CID_MPEG_VIDEOENC_AV1_ENABLE_TILE_GROUPS (V4L2_CID_MPEG_BASE + 598)
#endif

bool SetEncoderControl(NvVideoEncoder* encoder, uint32_t id, int32_t value) {
  v4l2_ext_control control = {};
  control.id = id;
  control.value = value;

  v4l2_ext_controls controls = {};
  controls.count = 1;
  controls.controls = &control;
  return encoder->setExtControls(controls) == 0;
}

}  // namespace

namespace livekit {

JetsonMmapiEncoder::JetsonMmapiEncoder(JetsonCodec codec) : codec_(codec) {}

JetsonMmapiEncoder::~JetsonMmapiEncoder() {
  Destroy();
}

bool JetsonMmapiEncoder::IsSupported() {
  return IsCodecSupported(JetsonCodec::kH264) ||
         IsCodecSupported(JetsonCodec::kH265) ||
         IsCodecSupported(JetsonCodec::kAV1);
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
  switch (codec) {
    case JetsonCodec::kH264:
      return V4L2_PIX_FMT_H264;
    case JetsonCodec::kH265:
      return V4L2_PIX_FMT_HEVC;
    case JetsonCodec::kAV1:
      return V4L2_PIX_FMT_AV1;
  }
  return V4L2_PIX_FMT_H264;
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
  if (initialized_) {
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
    return false;
  }

  if (!CreateEncoder()) {
    return false;
  }

  if (!ConfigureEncoder()) {
    return false;
  }

  if (!SetupPlanes()) {
    return false;
  }

  if (!QueueCaptureBuffers()) {
    return false;
  }

  if (!StartStreaming()) {
    return false;
  }

  initialized_ = true;
  RTC_LOG(LS_INFO) << "Jetson MMAPI encoder initialized: " << width_ << "x"
                   << height_ << " @ " << framerate_ << " fps";
  return true;
}

void JetsonMmapiEncoder::Destroy() {
  StopStreaming();
  if (encoder_) {
    delete encoder_;
    encoder_ = nullptr;
  }
  initialized_ = false;
  dmabuf_meta_cached_ = false;
  dmabuf_planes_setup_ = false;
  use_dmabuf_input_ = false;
  mmap_sync_supported_ = true;
  next_output_index_ = 0;
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
  if (!initialized_ || !encoder_) {
    return false;
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
  }

  if (!QueueOutputBuffer(src_y, stride_y, src_u, stride_u, src_v, stride_v)) {
    return false;
  }

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    return false;
  }

  if (!DequeueOutputBuffer()) {
    return false;
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
  if (!initialized_ || !encoder_) {
    return false;
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
  }

  if (!QueueOutputBufferNV12(src_y, stride_y, src_uv, stride_uv)) {
    return false;
  }

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    return false;
  }

  if (!DequeueOutputBuffer()) {
    return false;
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
  if (codec_ != JetsonCodec::kAV1) {
    encoder_->setIDRInterval(keyframe_interval_);
  }
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

void JetsonMmapiEncoder::ConfigureAv1HeadersWithFrame() {
  // WebRTC expects raw low-overhead OBUs. NVIDIA documents this control as
  // after both plane formats, but Jetson AV1 applies it before the output
  // format is set; otherwise IVF headers can still be attached.
  if (!SetEncoderControl(encoder_,
                         V4L2_CID_MPEG_VIDEOENC_AV1_HEADERS_WITH_FRAME, 0)) {
    RTC_LOG(LS_WARNING)
        << "Failed to disable AV1 IVF headers-with-frame on Jetson encoder; "
           "using driver default.";
  }
}

bool JetsonMmapiEncoder::ConfigureAv1Encoder() {
#ifdef V4L2_CID_MPEG_VIDEOENC_AV1_TILE_CONFIGURATION
  v4l2_enc_av1_tile_config tile_config = {};
  tile_config.bEnableTile = 0;
  tile_config.nLog2RowTiles = 0;
  tile_config.nLog2ColTiles = 0;
  if (encoder_->enableAV1Tile(tile_config) < 0) {
    RTC_LOG(LS_WARNING)
        << "Failed to configure AV1 single-tile mode; using driver default.";
  }
#endif

  // v1 intentionally stays single-tile and does not emit tile groups. Keep the
  // tile-group control best-effort because older JetPack drivers may not know
  // the AV1 extension even when compiling with newer headers.
  if (!SetEncoderControl(encoder_,
                         V4L2_CID_MPEG_VIDEOENC_AV1_ENABLE_TILE_GROUPS, 0)) {
    RTC_LOG(LS_WARNING)
        << "Failed to disable AV1 tile groups; using driver default.";
  }

  return true;
}

bool JetsonMmapiEncoder::ConfigureEncoder() {
  const uint32_t codec_pixfmt = CodecToV4L2PixFmt(codec_);
  const uint32_t bitstream_size =
      std::max(kMinBitstreamBufferSize, width_ * height_);

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
      return false;
    }
  }

  if (codec_ == JetsonCodec::kAV1) {
    ConfigureAv1HeadersWithFrame();
  }

  if (codec_ == JetsonCodec::kAV1) {
    // Argus supplies NV12 DMA buffers. Prefer NV12 for AV1 up front so the
    // DMABUF path does not change the output format after AV1 controls run.
    output_is_nv12_ = true;
    ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_, height_);
    if (ret < 0) {
      ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_YUV420M, width_,
                                           height_);
      if (ret < 0) {
        RTC_LOG(LS_ERROR) << "Failed to set AV1 output plane format.";
        return false;
      }
      output_is_nv12_ = false;
    }
  } else {
    // Prefer planar YUV420 (I420-style) for Jetson end-to-end.
    // If that fails, fall back to NV12M. The I420 input path can still be used
    // with NV12 by interleaving U/V into UV in QueueOutputBuffer().
    output_is_nv12_ = false;
    ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_YUV420M, width_,
                                         height_);
    if (ret < 0) {
      ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_,
                                           height_);
      if (ret < 0) {
        RTC_LOG(LS_ERROR) << "Failed to set output plane format.";
        return false;
      }
      output_is_nv12_ = true;
    }
  }

  if (codec_ == JetsonCodec::kAV1 && !ConfigureAv1Encoder()) {
    return false;
  }

  // These controls must be applied after both plane formats are set and before
  // either plane requests buffers. Keep them best-effort so older JetPack/MMAPI
  // versions can still encode if one low-latency knob is unavailable.
  encoder_->setMaxPerfMode(1);
  encoder_->setHWPresetType(V4L2_ENC_HW_PRESET_ULTRAFAST);
  encoder_->setNumBFrames(0);

  if (codec_ == JetsonCodec::kH264) {
    encoder_->setPocType(2);
  }

  encoder_->setBitrate(bitrate_bps_);
  encoder_->setFrameRate(framerate_, 1);
  encoder_->setRateControlMode(V4L2_MPEG_VIDEO_BITRATE_MODE_CBR);

  if (codec_ != JetsonCodec::kAV1) {
    encoder_->setIDRInterval(keyframe_interval_);
  }

  encoder_->setIFrameInterval(keyframe_interval_);

  if (codec_ != JetsonCodec::kAV1) {
    encoder_->setInsertSpsPpsAtIdrEnabled(true);
  }

  if (codec_ == JetsonCodec::kH264) {
    encoder_->setProfile(V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE);
    // Match the factory-advertised SDP profile-level-id (42e01f == CBP L3.1)
    // to avoid decoders rejecting due to an SPS level_idc higher than SDP.
    encoder_->setLevel(V4L2_MPEG_VIDEO_H264_LEVEL_3_1);
  } else if (codec_ == JetsonCodec::kH265) {
    encoder_->setProfile(V4L2_MPEG_VIDEO_H265_PROFILE_MAIN);
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
  NvBuffer* buffer = encoder_->output_plane.getNthBuffer(next_output_index_);
  if (!buffer) {
    RTC_LOG(LS_ERROR) << "Failed to get output buffer.";
    return false;
  }
  if (output_is_nv12_) {
    if (buffer->n_planes < 2) {
      RTC_LOG(LS_ERROR) << "Output plane format is NV12 but has <2 planes.";
      return false;
    }
  } else {
    if (buffer->n_planes < 3) {
      RTC_LOG(LS_ERROR) << "Output plane format is YUV420M but has <3 planes.";
      return false;
    }
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
  // Only probe NvBufSurface plane metadata if the API works for MMAP fds.
  // On JetPack versions where it doesn't, calling NvBufSurfaceFromFd
  // produces noisy "Wrong buffer index" warnings on every frame.
  const bool have_y = mmap_sync_supported_ &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[0].fd, 0, &y_pitch, &y_h, &y_np);
  const bool have_u = mmap_sync_supported_ &&
      (buffer->n_planes > 1) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[1].fd, 1, &u_pitch, &u_h, &u_np);
  const bool have_v = mmap_sync_supported_ &&
      (!output_is_nv12_ && buffer->n_planes > 2) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[2].fd, 2, &v_pitch, &v_h, &v_np);

  // Map the (sometimes unreliable) hardware metadata into plain-data hints so
  // the stride/height resolution stays pure and unit-testable. See
  // jetson_plane_layout.h.
  auto hints_for = [](const NvBuffer::NvBufferPlane& plane,
                      bool have_probe, uint32_t probed_pitch) {
    return PlaneLayoutHints{
        /*fmt_stride=*/static_cast<int>(plane.fmt.stride),
        /*probed_pitch=*/have_probe ? static_cast<int>(probed_pitch) : 0,
        /*plane_length=*/static_cast<int>(plane.length),
        /*fmt_height=*/static_cast<int>(plane.fmt.height)};
  };

  const int min_y_stride = width_;
  const int min_uv_stride = output_is_nv12_ ? width_ : chroma_width;
  const int dst_y_stride = ResolvePlaneStride(
      hints_for(buffer->planes[0], have_y, y_pitch), height_, min_y_stride,
      output_y_stride_);
  const int dst_u_stride = ResolvePlaneStride(
      hints_for(buffer->planes[1], have_u, u_pitch), chroma_height,
      min_uv_stride, output_u_stride_);
  const int dst_v_stride =
      (!output_is_nv12_ && buffer->n_planes > 2)
          ? ResolvePlaneStride(hints_for(buffer->planes[2], have_v, v_pitch),
                               chroma_height, chroma_width, output_v_stride_)
          : dst_u_stride;

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  const int plane_y_height =
      ResolvePlaneHeight(have_y, static_cast<int>(y_h), height_);
  const int plane_u_height =
      ResolvePlaneHeight(have_u, static_cast<int>(u_h), chroma_height);
  const int plane_v_height =
      ResolvePlaneHeight(have_v, static_cast<int>(v_h), chroma_height);
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

  // Best-effort sync of CPU-written pixel data to the device.  On JetPack
  // versions where NvBufSurfaceFromFd works for V4L2 MMAP plane fds this
  // flushes CPU caches before the hardware encoder reads the buffer.  On
  // versions where it doesn't (producing "Wrong buffer index" warnings),
  // we disable the sync after the first failure.  V4L2 MMAP + qBuffer
  // handles cache coherency on its own so skipping is safe.
  if (mmap_sync_supported_) {
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      int fd = buffer->planes[plane].fd;
      NvBufSurface* surface = nullptr;
      int map_ret =
          NvBufSurfaceFromFd(fd, reinterpret_cast<void**>(&surface));
      if (map_ret != 0 || !surface) {
        mmap_sync_supported_ = false;
        RTC_LOG(LS_WARNING)
            << "NvBufSurfaceFromFd failed for MMAP fd; disabling explicit "
               "device sync (V4L2 qBuffer handles coherency).";
        break;
      }
      int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, -1);
      if (sync_ret != 0) {
        mmap_sync_supported_ = false;
        RTC_LOG(LS_WARNING)
            << "NvBufSurfaceSyncForDevice failed for MMAP fd; disabling "
               "explicit device sync (V4L2 qBuffer handles coherency).";
        break;
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
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
  return true;
}

bool JetsonMmapiEncoder::QueueOutputBufferNV12(const uint8_t* src_y,
                                               int stride_y,
                                               const uint8_t* src_uv,
                                               int stride_uv) {
  if (!output_is_nv12_) {
    RTC_LOG(LS_ERROR) << "QueueOutputBufferNV12 called but output is not NV12.";
    return false;
  }

  NvBuffer* buffer = encoder_->output_plane.getNthBuffer(next_output_index_);
  if (!buffer) {
    RTC_LOG(LS_ERROR) << "Failed to get output buffer.";
    return false;
  }

  const int chroma_height = (height_ + 1) / 2;
  uint32_t y_pitch = 0, y_h = 0, y_np = 0;
  uint32_t uv_pitch = 0, uv_h = 0, uv_np = 0;
  const bool have_y = mmap_sync_supported_ &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[0].fd, 0, &y_pitch, &y_h, &y_np);
  const bool have_uv = mmap_sync_supported_ &&
      (buffer->n_planes > 1) &&
      GetPitchAndHeightFromNvBufSurfaceFd(buffer->planes[1].fd, 1, &uv_pitch, &uv_h, &uv_np);

  // See QueueOutputBuffer: keep the layout decisions in pure, testable helpers.
  auto hints_for = [](const NvBuffer::NvBufferPlane& plane,
                      bool have_probe, uint32_t probed_pitch) {
    return PlaneLayoutHints{
        /*fmt_stride=*/static_cast<int>(plane.fmt.stride),
        /*probed_pitch=*/have_probe ? static_cast<int>(probed_pitch) : 0,
        /*plane_length=*/static_cast<int>(plane.length),
        /*fmt_height=*/static_cast<int>(plane.fmt.height)};
  };

  const int min_y_stride = width_;
  const int min_uv_stride = width_;
  const int dst_y_stride = ResolvePlaneStride(
      hints_for(buffer->planes[0], have_y, y_pitch), height_, min_y_stride,
      output_y_stride_);
  const int dst_uv_stride = ResolvePlaneStride(
      hints_for(buffer->planes[1], have_uv, uv_pitch), chroma_height,
      min_uv_stride, output_u_stride_);

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
  const int plane_y_height =
      ResolvePlaneHeight(have_y, static_cast<int>(y_h), height_);
  const int plane_uv_height =
      ResolvePlaneHeight(have_uv, static_cast<int>(uv_h), chroma_height);
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

  // Best-effort device sync (see QueueOutputBuffer for rationale).
  if (mmap_sync_supported_) {
    for (int plane = 0; plane < buffer->n_planes; ++plane) {
      int fd = buffer->planes[plane].fd;
      NvBufSurface* surface = nullptr;
      int map_ret =
          NvBufSurfaceFromFd(fd, reinterpret_cast<void**>(&surface));
      if (map_ret != 0 || !surface) {
        mmap_sync_supported_ = false;
        RTC_LOG(LS_WARNING)
            << "NvBufSurfaceFromFd failed for MMAP fd; disabling explicit "
               "device sync (V4L2 qBuffer handles coherency).";
        break;
      }
      int sync_ret = NvBufSurfaceSyncForDevice(surface, 0, -1);
      if (sync_ret != 0) {
        mmap_sync_supported_ = false;
        RTC_LOG(LS_WARNING)
            << "NvBufSurfaceSyncForDevice failed for MMAP fd; disabling "
               "explicit device sync (V4L2 qBuffer handles coherency).";
        break;
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
  const bool verbose = std::getenv("LK_DUMP_H264_VERBOSE") != nullptr;
  constexpr int kMaxEmptyRetries = 5;
  constexpr int kDequeueTimeoutMs = 1000;
  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  NvBuffer* buffer = nullptr;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->capture_plane.getNumPlanes();

  size_t bytesused = 0;
  int empty_retries = 0;
  int backoff_ms = 1;
  for (int attempt = 0; attempt < kMaxEmptyRetries; ++attempt) {
    int dq_ret = encoder_->capture_plane.dqBuffer(v4l2_buf, &buffer, nullptr,
                                                  kDequeueTimeoutMs);
    if (dq_ret < 0) {
      RTC_LOG(LS_ERROR) << "Failed to dequeue capture buffer.";
      return false;
    }
    bytesused = v4l2_buf.m.planes[0].bytesused;
    if (bytesused > 0) {
      break;
    }
    empty_retries++;
    if (encoder_->capture_plane.qBuffer(v4l2_buf, nullptr) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to requeue empty capture buffer.";
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(backoff_ms));
    backoff_ms = std::min(backoff_ms * 2, 8);
  }

  if (bytesused == 0) {
    RTC_LOG(LS_WARNING) << "All " << kMaxEmptyRetries
                        << " dequeue attempts returned an empty buffer.";
    return false;
  }

  encoded->assign(static_cast<uint8_t*>(buffer->planes[0].data),
                  static_cast<uint8_t*>(buffer->planes[0].data) + bytesused);
  if (is_keyframe) {
    *is_keyframe = (v4l2_buf.flags & V4L2_BUF_FLAG_KEYFRAME) != 0;
  }
  if (verbose && verbose_left.load(std::memory_order_relaxed) > 0) {
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
    return false;
  }
  return true;
}

bool JetsonMmapiEncoder::DequeueOutputBuffer() {
  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = use_dmabuf_input_ ? V4L2_MEMORY_DMABUF : V4L2_MEMORY_MMAP;
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
  if (!initialized_ || !encoder_) {
    return false;
  }

  // On first DmaBuf encode, re-setup the output plane for V4L2_MEMORY_DMABUF.
  if (!dmabuf_planes_setup_) {
    // Stop streaming, reconfigure output plane, restart.
    StopStreaming();
    if (!SetupPlanesDmaBuf()) {
      return false;
    }
    if (!QueueCaptureBuffers()) {
      return false;
    }
    if (!StartStreaming()) {
      return false;
    }
    dmabuf_planes_setup_ = true;
    use_dmabuf_input_ = true;
    next_output_index_ = 0;
  }

  if (force_keyframe && !ForceKeyframe()) {
    RTC_LOG(LS_WARNING) << "Failed to request keyframe.";
  }

  if (!QueueOutputBufferDmaBuf(dmabuf_fd)) {
    return false;
  }

  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    return false;
  }

  if (!DequeueOutputBuffer()) {
    return false;
  }

  return true;
}

bool JetsonMmapiEncoder::SetupPlanesDmaBuf() {
  // The DMA buffers from Argus are NV12.  If the encoder was initially
  // configured for YUV420M (3-plane I420), reconfigure to NV12M so the
  // plane count matches the DMA buffer layout.
  if (!output_is_nv12_) {
    int ret = encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_, height_);
    if (ret < 0) {
      RTC_LOG(LS_ERROR)
          << "SetupPlanesDmaBuf: failed to switch output plane to NV12M.";
      return false;
    }
    output_is_nv12_ = true;
  }

  // Output plane uses V4L2_MEMORY_DMABUF: we request buffers but don't
  // allocate backing memory -- the caller provides DMA fds at queue time.
  output_buffer_count_ = kDefaultOutputBufferCount;
  capture_buffer_count_ = kDefaultCaptureBufferCount;

  if (encoder_->output_plane.setupPlane(V4L2_MEMORY_DMABUF,
                                        output_buffer_count_, false, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup output plane for DMABUF.";
    return false;
  }

  // Capture plane remains MMAP (encoded bitstream output).
  if (encoder_->capture_plane.setupPlane(V4L2_MEMORY_MMAP,
                                         capture_buffer_count_, true, false) <
      0) {
    RTC_LOG(LS_ERROR) << "Failed to setup capture plane.";
    return false;
  }

  return true;
}

bool JetsonMmapiEncoder::QueueOutputBufferDmaBuf(int dmabuf_fd) {
  // Cache the NvBufSurface plane metadata after the first successful lookup.
  // All DMA buffers in the ring share the same dimensions and NV12 layout,
  // so the pitch/height/num_planes values are constant.  Caching avoids
  // calling NvBufSurfaceFromFd on every frame, which on many JetPack
  // versions prints spurious "Wrong buffer index" warnings and adds latency.
  if (!dmabuf_meta_cached_) {
    NvBufSurface* surface = nullptr;
    int ret = NvBufSurfaceFromFd(dmabuf_fd, reinterpret_cast<void**>(&surface));
    if (ret != 0 || !surface) {
      RTC_LOG(LS_ERROR) << "QueueOutputBufferDmaBuf: NvBufSurfaceFromFd failed "
                        << "(fd=" << dmabuf_fd << ", ret=" << ret << ")";
      return false;
    }
    const NvBufSurfaceParams& params = surface->surfaceList[0];
    dmabuf_num_planes_ = params.planeParams.num_planes;
    for (uint32_t i = 0; i < dmabuf_num_planes_ && i < VIDEO_MAX_PLANES; ++i) {
      dmabuf_plane_bytesused_[i] =
          params.planeParams.pitch[i] * params.planeParams.height[i];
    }
    dmabuf_meta_cached_ = true;
  }

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_DMABUF;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();

  for (uint32_t i = 0; i < v4l2_buf.length && i < dmabuf_num_planes_; ++i) {
    planes[i].m.fd = dmabuf_fd;
    planes[i].bytesused = dmabuf_plane_bytesused_[i];
  }

  int qbuf_ret = encoder_->output_plane.qBuffer(v4l2_buf, nullptr);
  if (qbuf_ret < 0) {
    RTC_LOG(LS_ERROR) << "QueueOutputBufferDmaBuf: qBuffer failed "
                      << "(index=" << next_output_index_
                      << ", errno=" << errno << ": " << strerror(errno) << ")";
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
  return true;
}

bool JetsonMmapiEncoder::ForceKeyframe() {
  if (codec_ == JetsonCodec::kAV1) {
    // For AV1 the only control that actually forces an intra (key) frame on
    // this NVENC is V4L2_CID_MPEG_MFC51_VIDEO_FORCE_FRAME_TYPE, which is what
    // NvVideoEncoder::forceIDR() drives. The H.264/H.265-oriented
    // FORCE_IDR_FRAME / FORCE_INTRA_FRAME controls return success but are
    // silently ignored by the AV1 encoder, so the requested keyframe is never
    // produced and a receiver that PLIs can never recover.
    return encoder_->forceIDR() == 0;
  }

  v4l2_ext_control control = {};
  v4l2_ext_controls controls = {};
  controls.count = 1;
  controls.controls = &control;
  control.value = 1;
  control.id = V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME;
  return encoder_->setExtControls(controls) == 0;
}

}  // namespace livekit
