#include "jetson_h264_decoder.h"

#include <errno.h>
#include <sys/time.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <iterator>
#include <mutex>
#include <optional>
#include <utility>

#include "api/make_ref_counted.h"
#include "api/video/color_space.h"
#include "dmabuf_video_frame_buffer.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"

namespace webrtc {

namespace {

constexpr uint32_t kOutputBufferCount = 8;
constexpr uint32_t kCaptureBufferSlack = 4;
constexpr uint32_t kMaxBitstreamBufferSize = 4 * 1024 * 1024;
constexpr uint64_t kDrmFormatModifierLinear = 0;

constexpr uint32_t FourCc(char a, char b, char c, char d) {
  return static_cast<uint32_t>(a) | (static_cast<uint32_t>(b) << 8) |
         (static_cast<uint32_t>(c) << 16) | (static_cast<uint32_t>(d) << 24);
}

constexpr uint32_t kDrmFormatNv12 = FourCc('N', 'V', '1', '2');

uint32_t TimestampToRtp(const timeval& timestamp, uint32_t fallback) {
  if (timestamp.tv_sec == 0 && timestamp.tv_usec == 0) {
    return fallback;
  }
  return static_cast<uint32_t>(timestamp.tv_sec * 1000000ULL +
                               timestamp.tv_usec);
}

timeval RtpToTimestamp(uint32_t rtp_timestamp) {
  timeval timestamp;
  timestamp.tv_sec = rtp_timestamp / 1000000U;
  timestamp.tv_usec = rtp_timestamp % 1000000U;
  return timestamp;
}

struct CaptureBuffer {
  v4l2_buffer v4l2 = {};
  std::array<v4l2_plane, VIDEO_MAX_PLANES> planes = {};

  CaptureBuffer() {
    v4l2.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    v4l2.memory = V4L2_MEMORY_MMAP;
    v4l2.m.planes = planes.data();
  }

  CaptureBuffer(const v4l2_buffer& buffer,
                const std::array<v4l2_plane, VIDEO_MAX_PLANES>& buffer_planes)
      : v4l2(buffer), planes(buffer_planes) {
    v4l2.m.planes = planes.data();
  }

  void PrepareForQueue() {
    v4l2.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    v4l2.memory = V4L2_MEMORY_MMAP;
    v4l2.m.planes = planes.data();
    v4l2.bytesused = 0;
    for (uint32_t i = 0; i < v4l2.length && i < planes.size(); ++i) {
      planes[i].bytesused = 0;
    }
  }
};

livekit_ffi::DmaBufVideoFramePlane ToDmaBufPlane(const NvBuffer& buffer,
                                                 uint32_t plane_index) {
  const auto& plane = buffer.planes[plane_index];
  return livekit_ffi::DmaBufVideoFramePlane{
      .fd = plane.fd,
      .offset = plane.mem_offset,
      .stride = plane.fmt.stride,
      .size = plane.length,
      .width = plane.fmt.width,
      .height = plane.fmt.height,
  };
}

livekit_ffi::DmaBufVideoFrameDescriptor ToDmaBufDescriptor(
    const NvBuffer& buffer,
    uint32_t visible_width,
    uint32_t visible_height) {
  return livekit_ffi::DmaBufVideoFrameDescriptor{
      .width = visible_width,
      .height = visible_height,
      .fourcc = kDrmFormatNv12,
      .modifier = kDrmFormatModifierLinear,
      .num_planes = std::min<uint32_t>(buffer.n_planes, 2),
      .y = ToDmaBufPlane(buffer, 0),
      .uv = ToDmaBufPlane(buffer, 1),
  };
}

}  // namespace

class JetsonH264Decoder::State {
 public:
  void RequeueCaptureBuffer(CaptureBuffer capture) {
    std::lock_guard<std::mutex> lock(mutex_);
    if (closing_ || !decoder || !capture_configured) {
      return;
    }

    capture.PrepareForQueue();
    if (decoder->capture_plane.qBuffer(capture.v4l2, nullptr) < 0) {
      RTC_LOG(LS_WARNING) << "Failed to requeue Jetson capture buffer";
    }
  }

  void Close() {
    std::lock_guard<std::mutex> lock(mutex_);
    closing_ = true;
    if (!decoder) {
      return;
    }

    decoder->output_plane.setStreamStatus(false);
    if (capture_configured) {
      decoder->capture_plane.setStreamStatus(false);
    }
  }

  std::mutex& mutex() { return mutex_; }
  bool closing() const { return closing_; }

  std::unique_ptr<NvVideoDecoder> decoder;
  bool capture_configured = false;
  uint32_t width = 0;
  uint32_t height = 0;

 private:
  std::mutex mutex_;
  bool closing_ = false;
};

JetsonH264Decoder::JetsonH264Decoder() = default;

JetsonH264Decoder::~JetsonH264Decoder() {
  Release();
}

bool JetsonH264Decoder::Configure(const Settings& settings) {
  if (settings.codec_type() != kVideoCodecH264) {
    RTC_LOG(LS_ERROR) << "Jetson decoder only supports H264";
    return false;
  }
  if (!settings.max_render_resolution().Valid()) {
    RTC_LOG(LS_ERROR) << "Invalid Jetson decoder render resolution";
    return false;
  }

  auto state = std::make_shared<State>();
  state->decoder.reset(NvVideoDecoder::createVideoDecoder("livekit_h264"));
  if (!state->decoder) {
    RTC_LOG(LS_ERROR) << "Failed to create Jetson V4L2 decoder";
    return false;
  }

  if (state->decoder->subscribeEvent(V4L2_EVENT_RESOLUTION_CHANGE, 0, 0) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to subscribe to Jetson resolution changes";
    return false;
  }

  state->decoder->setMaxPerfMode(1);

  if (state->decoder->setOutputPlaneFormat(V4L2_PIX_FMT_H264,
                                           kMaxBitstreamBufferSize) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to configure Jetson output plane format";
    return false;
  }

  if (state->decoder->output_plane.setupPlane(V4L2_MEMORY_MMAP,
                                              kOutputBufferCount, true,
                                              false) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set up Jetson output plane";
    return false;
  }

  if (state->decoder->output_plane.setStreamStatus(true) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start Jetson output plane";
    return false;
  }

  free_output_buffers_ = {};
  for (uint32_t i = 0; i < state->decoder->output_plane.getNumBuffers(); ++i) {
    free_output_buffers_.push(i);
  }

  settings_ = settings;
  state_ = std::move(state);
  return true;
}

int32_t JetsonH264Decoder::RegisterDecodeCompleteCallback(
    DecodedImageCallback* callback) {
  decoded_complete_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264Decoder::Release() {
  if (state_) {
    state_->Close();
    state_.reset();
  }
  free_output_buffers_ = {};
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoDecoder::DecoderInfo JetsonH264Decoder::GetDecoderInfo() const {
  VideoDecoder::DecoderInfo info;
  info.implementation_name = "Jetson H264 Decoder";
  info.is_hardware_accelerated = true;
  return info;
}

int32_t JetsonH264Decoder::Decode(const EncodedImage& input_image,
                                  bool missing_frames,
                                  int64_t render_time_ms) {
  if (!state_ || !state_->decoder) {
    RTC_LOG(LS_ERROR) << "Jetson decoder is not configured";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!decoded_complete_callback_) {
    RTC_LOG(LS_ERROR) << "Jetson decode callback is not configured";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!input_image.data() || input_image.size() == 0) {
    RTC_LOG(LS_ERROR) << "Jetson decoder received an empty input image";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  h264_bitstream_parser_.ParseBitstream(input_image);
  const std::optional<int> qp = h264_bitstream_parser_.GetLastSliceQp();
  const ColorSpace color_space =
      input_image.ColorSpace() ? *input_image.ColorSpace() : ColorSpace();

  std::vector<VideoFrame> decoded_frames;
  {
    std::lock_guard<std::mutex> lock(state_->mutex());
    if (state_->closing()) {
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    }

    if (!QueueInputBuffer(input_image)) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    DrainOutputPlane();
    PollResolutionChange();
    auto frames =
        DrainCapturePlane(input_image.RtpTimestamp(), color_space);
    decoded_frames.insert(decoded_frames.end(), std::make_move_iterator(frames.begin()),
                          std::make_move_iterator(frames.end()));
  }

  for (auto& decoded_frame : decoded_frames) {
    std::optional<int32_t> decodetime;
    decoded_complete_callback_->Decoded(decoded_frame, decodetime, qp);
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

bool JetsonH264Decoder::QueueInputBuffer(const EncodedImage& input_image) {
  DrainOutputPlane();
  if (free_output_buffers_.empty()) {
    RTC_LOG(LS_WARNING) << "Jetson output plane has no free buffers";
    return false;
  }

  const uint32_t index = free_output_buffers_.front();
  free_output_buffers_.pop();
  NvBuffer* buffer = state_->decoder->output_plane.getNthBuffer(index);
  if (!buffer || buffer->n_planes == 0 || !buffer->planes[0].data) {
    RTC_LOG(LS_ERROR) << "Jetson output buffer is invalid";
    return false;
  }
  if (input_image.size() > buffer->planes[0].length) {
    RTC_LOG(LS_ERROR) << "Encoded H264 frame is larger than Jetson output buffer";
    free_output_buffers_.push(index);
    return false;
  }

  std::memcpy(buffer->planes[0].data, input_image.data(), input_image.size());
  buffer->planes[0].bytesused = input_image.size();

  v4l2_buffer v4l2_buf = {};
  std::array<v4l2_plane, VIDEO_MAX_PLANES> planes = {};
  v4l2_buf.index = index;
  v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
  v4l2_buf.memory = V4L2_MEMORY_MMAP;
  v4l2_buf.length = buffer->n_planes;
  v4l2_buf.m.planes = planes.data();
  v4l2_buf.flags |= V4L2_BUF_FLAG_TIMESTAMP_COPY;
  v4l2_buf.timestamp = RtpToTimestamp(input_image.RtpTimestamp());
  planes[0].bytesused = input_image.size();

  if (state_->decoder->output_plane.qBuffer(v4l2_buf, nullptr) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to queue Jetson output buffer";
    free_output_buffers_.push(index);
    return false;
  }

  return true;
}

void JetsonH264Decoder::DrainOutputPlane() {
  while (true) {
    v4l2_buffer v4l2_buf = {};
    std::array<v4l2_plane, VIDEO_MAX_PLANES> planes = {};
    NvBuffer* buffer = nullptr;
    v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE;
    v4l2_buf.memory = V4L2_MEMORY_MMAP;
    v4l2_buf.length = planes.size();
    v4l2_buf.m.planes = planes.data();

    const int ret =
        state_->decoder->output_plane.dqBuffer(v4l2_buf, &buffer, nullptr, 0);
    if (ret < 0) {
      if (errno != EAGAIN) {
        RTC_LOG(LS_WARNING) << "Failed to dequeue Jetson output buffer";
      }
      return;
    }

    free_output_buffers_.push(v4l2_buf.index);
  }
}

bool JetsonH264Decoder::PollResolutionChange() {
  bool changed = false;
  while (true) {
    v4l2_event event = {};
    const int ret = state_->decoder->dqEvent(event, 0);
    if (ret < 0) {
      if (errno != EAGAIN) {
        RTC_LOG(LS_WARNING) << "Failed to dequeue Jetson decoder event";
      }
      return changed;
    }
    if (event.type == V4L2_EVENT_RESOLUTION_CHANGE) {
      changed = ConfigureCapturePlane();
    }
  }
}

bool JetsonH264Decoder::ConfigureCapturePlane() {
  if (state_->capture_configured) {
    state_->decoder->capture_plane.setStreamStatus(false);
    state_->decoder->capture_plane.deinitPlane();
    state_->capture_configured = false;
  }

  v4l2_format format = {};
  format.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
  if (state_->decoder->capture_plane.getFormat(format) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to read Jetson capture plane format";
    return false;
  }

  const uint32_t width = format.fmt.pix_mp.width;
  const uint32_t height = format.fmt.pix_mp.height;
  if (width == 0 || height == 0) {
    RTC_LOG(LS_ERROR) << "Jetson capture plane reported an empty frame size";
    return false;
  }

  if (state_->decoder->setCapturePlaneFormat(V4L2_PIX_FMT_NV12M, width,
                                             height) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to configure Jetson capture plane format";
    return false;
  }

  int min_buffers = 0;
  if (state_->decoder->getMinimumCapturePlaneBuffers(min_buffers) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to read Jetson capture plane buffer count";
    return false;
  }

  const uint32_t capture_buffers =
      static_cast<uint32_t>(min_buffers) + kCaptureBufferSlack;
  if (state_->decoder->capture_plane.setupPlane(V4L2_MEMORY_MMAP,
                                                capture_buffers, false,
                                                false) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to set up Jetson capture plane";
    return false;
  }

  for (uint32_t i = 0; i < state_->decoder->capture_plane.getNumBuffers();
       ++i) {
    if (state_->decoder->capture_plane.exportBuffer(i) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to export Jetson capture buffer";
      return false;
    }
  }

  if (state_->decoder->capture_plane.setStreamStatus(true) < 0) {
    RTC_LOG(LS_ERROR) << "Failed to start Jetson capture plane";
    return false;
  }

  for (uint32_t i = 0; i < state_->decoder->capture_plane.getNumBuffers();
       ++i) {
    NvBuffer* buffer = state_->decoder->capture_plane.getNthBuffer(i);
    if (!buffer) {
      RTC_LOG(LS_ERROR) << "Jetson capture buffer is invalid";
      return false;
    }

    CaptureBuffer capture;
    capture.v4l2.index = i;
    capture.v4l2.length = buffer->n_planes;
    capture.PrepareForQueue();
    if (state_->decoder->capture_plane.qBuffer(capture.v4l2, nullptr) < 0) {
      RTC_LOG(LS_ERROR) << "Failed to queue Jetson capture buffer";
      return false;
    }
  }

  state_->width = width;
  state_->height = height;
  state_->capture_configured = true;
  RTC_LOG(LS_INFO) << "Configured Jetson capture plane: " << width << "x"
                   << height << ", buffers=" << capture_buffers;
  return true;
}

std::vector<VideoFrame> JetsonH264Decoder::DrainCapturePlane(
    uint32_t fallback_rtp_timestamp,
    const ColorSpace& color_space) {
  std::vector<VideoFrame> frames;
  if (!state_->capture_configured) {
    return frames;
  }

  while (true) {
    v4l2_buffer v4l2_buf = {};
    std::array<v4l2_plane, VIDEO_MAX_PLANES> planes = {};
    NvBuffer* buffer = nullptr;
    v4l2_buf.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
    v4l2_buf.memory = V4L2_MEMORY_MMAP;
    v4l2_buf.length = planes.size();
    v4l2_buf.m.planes = planes.data();

    const int ret =
        state_->decoder->capture_plane.dqBuffer(v4l2_buf, &buffer, nullptr, 0);
    if (ret < 0) {
      if (errno != EAGAIN) {
        RTC_LOG(LS_WARNING) << "Failed to dequeue Jetson capture buffer";
      }
      return frames;
    }
    CaptureBuffer capture(v4l2_buf, planes);
    if (!buffer || buffer->n_planes < 2) {
      RTC_LOG(LS_WARNING) << "Jetson capture buffer is not NV12";
      capture.PrepareForQueue();
      if (state_->decoder->capture_plane.qBuffer(capture.v4l2, nullptr) < 0) {
        RTC_LOG(LS_WARNING) << "Failed to requeue invalid Jetson capture buffer";
      }
      continue;
    }

    const uint32_t rtp_timestamp =
        TimestampToRtp(v4l2_buf.timestamp, fallback_rtp_timestamp);
    auto state = state_;
    auto frame_buffer =
        webrtc::make_ref_counted<livekit_ffi::DmaBufVideoFrameBuffer>(
            ToDmaBufDescriptor(*buffer, state_->width, state_->height),
            [state, capture = std::move(capture)]() mutable {
              if (state) {
                state->RequeueCaptureBuffer(std::move(capture));
              }
            });

    frames.push_back(VideoFrame::Builder()
                         .set_video_frame_buffer(frame_buffer)
                         .set_timestamp_rtp(rtp_timestamp)
                         .set_color_space(color_space)
                         .build());
  }
}

}  // namespace webrtc
