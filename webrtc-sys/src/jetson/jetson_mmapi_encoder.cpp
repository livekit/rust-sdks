#include "jetson_mmapi_encoder.h"

#include <linux/v4l2-controls.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <cstring>
#include <memory>

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
  if (!QueueOutputBuffer(src_y, stride_y, src_uv, stride_uv)) {
    return false;
  }
  if (!DequeueCaptureBuffer(encoded, is_keyframe)) {
    return false;
  }
  return DequeueOutputBuffer();
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
  const uint32_t codec_pixfmt = CodecToV4L2PixFmt(codec_);
  const uint32_t bitstream_size =
      std::max(kMinBitstreamBufferSize, width_ * height_);

  if (encoder_->setOutputPlaneFormat(V4L2_PIX_FMT_NV12M, width_, height_) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set output plane format.";
    return false;
  }
  if (encoder_->setCapturePlaneFormat(codec_pixfmt, width_, height_,
                                      bitstream_size) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set capture plane format.";
    return false;
  }
  encoder_->setBitrate(bitrate_bps_);
  encoder_->setFrameRate(framerate_, 1);
  encoder_->setRateControlMode(V4L2_MPEG_VIDEO_BITRATE_MODE_CBR);
  encoder_->setIDRInterval(keyframe_interval_);
  encoder_->setIFrameInterval(keyframe_interval_);
  encoder_->setInsertSpsPpsAtIdrEnabled(true);

  if (codec_ == JetsonCodec::kH264) {
    encoder_->setProfile(V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE);
    encoder_->setLevel(V4L2_MPEG_VIDEO_H264_LEVEL_5_0);
  } else {
    encoder_->setProfile(V4L2_MPEG_VIDEO_H265_PROFILE_MAIN);
  }

  v4l2_format output_format = {};
  if (encoder_->output_plane.getFormat(output_format) == 0) {
    output_y_stride_ =
        output_format.fmt.pix_mp.plane_fmt[0].bytesperline;
    output_uv_stride_ =
        output_format.fmt.pix_mp.plane_fmt[1].bytesperline;
  }
  if (output_y_stride_ == 0) {
    output_y_stride_ = width_;
  }
  if (output_uv_stride_ == 0) {
    output_uv_stride_ = width_;
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
                                           const uint8_t* src_uv,
                                           int stride_uv) {
  NvBuffer* buffer = encoder_->output_plane.getNthBuffer(next_output_index_);
  if (!buffer) {
    RTC_LOG(LS_ERROR) << "Failed to get output buffer.";
    return false;
  }

  uint8_t* dst_y = static_cast<uint8_t*>(buffer->planes[0].data);
  uint8_t* dst_uv = static_cast<uint8_t*>(buffer->planes[1].data);
  CopyPlane(dst_y, output_y_stride_, src_y, stride_y, width_, height_);
  CopyPlane(dst_uv, output_uv_stride_, src_uv, stride_uv, width_, height_ / 2);

  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  v4l2_buf.index = next_output_index_;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->output_plane.getNumPlanes();
  planes[0].bytesused = width_ * height_;
  planes[1].bytesused = width_ * height_ / 2;

  if (encoder_->output_plane.qBuffer(v4l2_buf, nullptr) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to queue output buffer.";
    return false;
  }

  next_output_index_ = (next_output_index_ + 1) % output_buffer_count_;
  return true;
}

bool JetsonMmapiEncoder::DequeueCaptureBuffer(std::vector<uint8_t>* encoded,
                                              bool* is_keyframe) {
  v4l2_buffer v4l2_buf = {};
  v4l2_plane planes[VIDEO_MAX_PLANES] = {};
  NvBuffer* buffer = nullptr;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.m.planes = planes;
  v4l2_buf.length = encoder_->capture_plane.getNumPlanes();

  if (encoder_->capture_plane.dqBuffer(v4l2_buf, &buffer, nullptr, 0) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to dequeue capture buffer.";
    return false;
  }

  const size_t bytesused = v4l2_buf.m.planes[0].bytesused;
  encoded->assign(static_cast<uint8_t*>(buffer->planes[0].data),
                  static_cast<uint8_t*>(buffer->planes[0].data) + bytesused);
  if (is_keyframe) {
    *is_keyframe = (v4l2_buf.flags & V4L2_BUF_FLAG_KEYFRAME) != 0;
  }

  if (encoder_->capture_plane.qBuffer(v4l2_buf, nullptr) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to requeue capture buffer.";
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
