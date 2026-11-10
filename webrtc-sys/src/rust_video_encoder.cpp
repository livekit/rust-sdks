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

#include "livekit/rust_video_encoder.h"

#include <algorithm>
#include <cmath>
#include <utility>

#include "api/video/encoded_image.h"
#include "api/video/i420_buffer.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/rust_video_encoder.rs.h"

#ifdef LIVEKIT_TEST
#include <atomic>
#include <chrono>
#include <thread>

#include "api/environment/environment_factory.h"
#include "livekit/video_encoder_factory.h"
#include "livekit/webrtc.h"
#endif

namespace livekit_ffi {

namespace {

// More than the encoder ever holds.
constexpr size_t kMaxExpectedFrames = 16;

webrtc::SdpVideoFormat H264Format() {
  return webrtc::SdpVideoFormat("H264", {{"profile-level-id", "42e01f"},
                                         {"level-asymmetry-allowed", "1"},
                                         {"packetization-mode", "1"}});
}

struct OfferedCodec {
  CodecKind kind;
  webrtc::SdpVideoFormat format;
};

// A new codec also needs a CodecKind and its CodecSpecificInfo in deliver().
const std::vector<OfferedCodec>& OfferedCodecs() {
  static const std::vector<OfferedCodec> codecs = {
      {CodecKind::H264, H264Format()},
  };
  return codecs;
}

class RustVideoEncoderAdapter final : public webrtc::VideoEncoder {
 public:
  explicit RustVideoEncoderAdapter(rust::Box<RustVideoEncoder> encoder)
      : encoder_(std::move(encoder)) {}

  // WebRTC may destroy an encoder without releasing it.
  ~RustVideoEncoderAdapter() override { DetachSink(); }

  int InitEncode(const webrtc::VideoCodec* codec_settings,
                 const Settings& /* settings */) override {
    if (!codec_settings) {
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }
    DetachSink();
    sink_ = std::make_shared<EncodedSink>(callback_, codec_settings->codecType,
                                          codec_settings->width,
                                          codec_settings->height);
    failure_logged_ = false;
    const EncoderInit init{
        .width = codec_settings->width,
        .height = codec_settings->height,
        .bitrate_bps = codec_settings->startBitrate * 1000,
        .framerate = codec_settings->maxFramerate,
    };
    if (!encoder_->init(init, sink_)) {
      DetachSink();
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override {
    callback_ = callback;
    if (sink_) {
      sink_->SetCallback(callback);
    }
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Release() override {
    DetachSink();
    encoder_->release();
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* frame_types) override {
    if (!sink_) {
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    }
    const bool keyframe =
        frame_types &&
        std::find(frame_types->begin(), frame_types->end(),
                  webrtc::VideoFrameType::kVideoFrameKey) != frame_types->end();
    const FrameInfo info{
        .rtp_timestamp = frame.rtp_timestamp(),
        .capture_time_ms = frame.render_time_ms(),
    };
    sink_->Expect(frame);
    switch (encoder_->encode(
        std::make_unique<VideoFrameRef>(frame.video_frame_buffer()), info,
        keyframe)) {
      case EncodeStatus::Queued:
        return WEBRTC_VIDEO_CODEC_OK;
      case EncodeStatus::Dropped:
        sink_->dropped(info);
        return WEBRTC_VIDEO_CODEC_OK;
      default:
        if (!std::exchange(failure_logged_, true)) {
          RTC_LOG(LS_ERROR) << ImplementationName()
                            << " encoder failed; falling back to software "
                               "encoding. The encoder's own log has the cause.";
        }
        return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }
  }

  void SetRates(const RateControlParameters& parameters) override {
    encoder_->set_rates(
        parameters.bitrate.get_sum_bps(),
        static_cast<uint32_t>(std::lround(parameters.framerate_fps)));
  }

  EncoderInfo GetEncoderInfo() const override {
    EncoderInfo info;
    info.implementation_name = ImplementationName();
    info.is_hardware_accelerated = true;
    info.supports_native_handle = false;
    info.supports_simulcast = false;
    info.scaling_settings = ScalingSettings::kOff;
    // 4:2:0 chroma needs even dimensions.
    info.requested_resolution_alignment = 2;
    return info;
  }

 private:
  std::string ImplementationName() const {
    return std::string(encoder_->implementation_name());
  }

  void DetachSink() {
    if (sink_) {
      sink_->Detach();
      sink_.reset();
    }
  }

  rust::Box<RustVideoEncoder> encoder_;
  std::shared_ptr<EncodedSink> sink_;
  webrtc::EncodedImageCallback* callback_ = nullptr;
  bool failure_logged_ = false;
};

}  // namespace

I420View::I420View(webrtc::scoped_refptr<webrtc::I420BufferInterface> buffer)
    : buffer_(std::move(buffer)) {}

uint32_t I420View::width() const {
  return buffer_->width();
}

uint32_t I420View::height() const {
  return buffer_->height();
}

rust::Slice<const uint8_t> I420View::plane(size_t index) const {
  const size_t chroma_rows = buffer_->ChromaHeight();
  switch (index) {
    case 0:
      return {buffer_->DataY(),
              static_cast<size_t>(buffer_->StrideY()) * buffer_->height()};
    case 1:
      return {buffer_->DataU(),
              static_cast<size_t>(buffer_->StrideU()) * chroma_rows};
    default:
      return {buffer_->DataV(),
              static_cast<size_t>(buffer_->StrideV()) * chroma_rows};
  }
}

uint32_t I420View::stride(size_t index) const {
  switch (index) {
    case 0:
      return buffer_->StrideY();
    case 1:
      return buffer_->StrideU();
    default:
      return buffer_->StrideV();
  }
}

VideoFrameRef::VideoFrameRef(
    webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer)
    : buffer_(std::move(buffer)) {}

std::unique_ptr<I420View> VideoFrameRef::to_i420() const {
  auto i420 = buffer_->ToI420();
  if (!i420) {
    return nullptr;
  }
  return std::make_unique<I420View>(std::move(i420));
}

EncodedSink::EncodedSink(webrtc::EncodedImageCallback* callback,
                         webrtc::VideoCodecType codec,
                         uint32_t width,
                         uint32_t height)
    : codec_(codec), width_(width), height_(height), callback_(callback) {}

void EncodedSink::SetCallback(webrtc::EncodedImageCallback* callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = callback;
}

void EncodedSink::Detach() {
  SetCallback(nullptr);
}

void EncodedSink::Expect(const webrtc::VideoFrame& frame) {
  webrtc::MutexLock lock(&mutex_);
  expected_.push_back(Expected{frame.rtp_timestamp(), frame.ntp_time_ms(),
                               frame.rotation(), frame.color_space()});
  if (expected_.size() > kMaxExpectedFrames) {
    expected_.pop_front();
  }
}

std::optional<EncodedSink::Expected> EncodedSink::Take(
    uint32_t rtp_timestamp) const {
  auto it = std::find_if(expected_.begin(), expected_.end(),
                         [rtp_timestamp](const Expected& expected) {
                           return expected.rtp_timestamp == rtp_timestamp;
                         });
  if (it == expected_.end()) {
    return std::nullopt;
  }
  Expected expected = std::move(*it);
  expected_.erase(it);
  return expected;
}

void EncodedSink::deliver(rust::Slice<const uint8_t> data,
                          const FrameInfo& info,
                          bool keyframe) const {
  webrtc::MutexLock lock(&mutex_);
  auto expected = Take(info.rtp_timestamp);
  if (!callback_) {
    return;
  }

  webrtc::EncodedImage image;
  image.SetEncodedData(
      webrtc::EncodedImageBuffer::Create(data.data(), data.size()));
  image._encodedWidth = width_;
  image._encodedHeight = height_;
  image.SetRtpTimestamp(info.rtp_timestamp);
  image.capture_time_ms_ = info.capture_time_ms;
  image._frameType = keyframe ? webrtc::VideoFrameType::kVideoFrameKey
                              : webrtc::VideoFrameType::kVideoFrameDelta;
  image.content_type_ = webrtc::VideoContentType::UNSPECIFIED;
  image.timing_.flags = webrtc::VideoSendTiming::kInvalid;
  if (expected) {
    image.ntp_time_ms_ = expected->ntp_time_ms;
    image.rotation_ = expected->rotation;
    image.SetColorSpace(expected->color_space);
  }

  webrtc::CodecSpecificInfo codec_info;
  codec_info.codecType = codec_;
  if (codec_ == webrtc::kVideoCodecH264) {
    h264_parser_.ParseBitstream(image);
    image.qp_ = h264_parser_.GetLastSliceQp().value_or(-1);
    codec_info.codecSpecific.H264.packetization_mode =
        webrtc::H264PacketizationMode::NonInterleaved;
    codec_info.codecSpecific.H264.temporal_idx = webrtc::kNoTemporalIdx;
    codec_info.codecSpecific.H264.base_layer_sync = false;
    codec_info.codecSpecific.H264.idr_frame = keyframe;
  }
  // A failed send (e.g. a layer paused by dynacast) is not an encoder error.
  callback_->OnEncodedImage(image, &codec_info);
}

void EncodedSink::dropped(const FrameInfo& info) const {
  webrtc::MutexLock lock(&mutex_);
  Take(info.rtp_timestamp);
  if (callback_) {
    callback_->OnFrameDropped(info.rtp_timestamp, /*spatial_id=*/0,
                              /*is_end_of_temporal_unit=*/true);
  }
}

RustVideoEncoderFactory::RustVideoEncoderFactory(
    rust::Box<RustEncoderFactory> factory)
    : factory_(std::move(factory)) {}

RustVideoEncoderFactory::~RustVideoEncoderFactory() = default;

std::vector<webrtc::SdpVideoFormat>
RustVideoEncoderFactory::GetSupportedFormats() const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& codec : OfferedCodecs()) {
    if (factory_->supports(codec.kind)) {
      formats.push_back(codec.format);
    }
  }
  return formats;
}

std::unique_ptr<webrtc::VideoEncoder> RustVideoEncoderFactory::Create(
    const webrtc::Environment& /* env */,
    const webrtc::SdpVideoFormat& format) {
  for (const auto& codec : OfferedCodecs()) {
    if (codec.format.name != format.name) {
      continue;
    }
    auto encoder = factory_->create_encoder(codec.kind);
    // The device has failed too often; let the caller pick another encoder.
    if (!encoder->usable()) {
      return nullptr;
    }
    return std::make_unique<RustVideoEncoderAdapter>(std::move(encoder));
  }
  return nullptr;
}

std::unique_ptr<webrtc::VideoEncoderFactory> CreateV4l2VideoEncoderFactory() {
  auto factory =
      std::make_unique<RustVideoEncoderFactory>(new_v4l2_encoder_factory());
  if (factory->GetSupportedFormats().empty()) {
    return nullptr;
  }
  return factory;
}

bool HasV4l2VideoEncoder() {
  static const bool available = CreateV4l2VideoEncoderFactory() != nullptr;
  return available;
}

#ifdef LIVEKIT_TEST
namespace {

class CountingCallback final : public webrtc::EncodedImageCallback {
 public:
  Result OnEncodedImage(const webrtc::EncodedImage& /* image */,
                        const webrtc::CodecSpecificInfo* /* info */) override {
    ++sent;
    return Result(Result::OK);
  }
  void OnFrameDropped(uint32_t /* rtp_timestamp */,
                      int /* spatial_id */,
                      bool /* is_end_of_temporal_unit */) override {}
  std::atomic<uint32_t> sent{0};
};

constexpr char kMissingDevice[] = "/nonexistent/video0";

webrtc::VideoCodec TestCodec() {
  webrtc::VideoCodec codec;
  codec.codecType = webrtc::kVideoCodecH264;
  codec.width = 320;
  codec.height = 240;
  codec.maxFramerate = 30;
  codec.startBitrate = 300;
  codec.maxBitrate = 600;
  codec.qpMax = 51;
  *codec.H264() = webrtc::VideoEncoder::GetDefaultH264Settings();
  return codec;
}

webrtc::VideoEncoder::Settings TestSettings() {
  return webrtc::VideoEncoder::Settings(
      webrtc::VideoEncoder::Capabilities(false), 1, 1200);
}

webrtc::VideoFrame TestFrame(uint32_t index) {
  auto buffer = webrtc::I420Buffer::Create(320, 240);
  webrtc::I420Buffer::SetBlack(buffer.get());
  return webrtc::VideoFrame::Builder()
      .set_video_frame_buffer(buffer)
      .set_rtp_timestamp(index * 3000)
      .build();
}

}  // namespace

rust::Vec<rust::String> rust_encoder_fallback_for_test() {
  VideoEncoderFactory factory;
  factory.AddBackendForTest(VideoEncoderBackendFactory{
      VideoEncoderBackend::Hardware,
      std::make_unique<RustVideoEncoderFactory>(
          new_v4l2_encoder_factory_for_test(kMissingDevice)),
      /*software_fallback=*/true});
  auto encoder = factory.Create(webrtc::CreateEnvironment(), H264Format());

  const webrtc::VideoCodec codec = TestCodec();
  encoder->InitEncode(&codec, TestSettings());
  CountingCallback callback;
  encoder->RegisterEncodeCompleteCallback(&callback);
  webrtc::VideoBitrateAllocation allocation;
  allocation.SetBitrate(0, 0, 300'000);
  encoder->SetRates(
      webrtc::VideoEncoder::RateControlParameters(allocation, 30.0));

  const std::vector<webrtc::VideoFrameType> frame_types{
      webrtc::VideoFrameType::kVideoFrameDelta};
  rust::Vec<rust::String> implementations;
  // Frames sent before the thread finds the device missing are lost.
  for (uint32_t i = 1; i <= 500 && implementations.size() < 3; ++i) {
    const uint32_t sent_before = callback.sent;
    encoder->Encode(TestFrame(i), &frame_types);
    if (callback.sent > sent_before) {
      implementations.push_back(encoder->GetEncoderInfo().implementation_name);
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  encoder->Release();
  return implementations;
}

void rust_encoder_lifecycle_for_test(uint32_t cycles) {
  const auto env = webrtc::CreateEnvironment();
  const webrtc::VideoCodec codec = TestCodec();
  CountingCallback callback;
  for (uint32_t cycle = 0; cycle < cycles; ++cycle) {
    // A new factory each time: one gives up on a device that keeps failing.
    RustVideoEncoderFactory factory(
        new_v4l2_encoder_factory_for_test(kMissingDevice));
    auto encoder = factory.Create(env, H264Format());
    encoder->RegisterEncodeCompleteCallback(&callback);
    encoder->InitEncode(&codec, TestSettings());
    for (uint32_t i = 1; i <= 3; ++i) {
      encoder->Encode(TestFrame(i), nullptr);
    }
    switch (cycle % 3) {
      case 0:
        encoder->Release();
        break;
      case 1:
        encoder->InitEncode(&codec, TestSettings());
        encoder->Encode(TestFrame(4), nullptr);
        break;
      default:
        break;
    }
    encoder.reset();
    // The factory joins the encoder's threads.
  }
}

uint32_t encoded_sink_detach_for_test() {
  CountingCallback callback;
  EncodedSink sink(&callback, webrtc::kVideoCodecH264, 16, 16);
  const uint8_t keyframe[] = {0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1,
                              0x68, 0,  0,  0, 1,    0x65, 0x88};
  const rust::Slice<const uint8_t> data(keyframe, sizeof(keyframe));
  sink.deliver(data, FrameInfo{.rtp_timestamp = 1, .capture_time_ms = 0},
               true);
  sink.Detach();
  sink.deliver(data, FrameInfo{.rtp_timestamp = 2, .capture_time_ms = 0},
               true);
  return callback.sent;
}
#endif

}  // namespace livekit_ffi
