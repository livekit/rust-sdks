#include <gtest/gtest.h>

#include <algorithm>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <mutex>
#include <string>
#include <vector>

#include "api/environment/environment_factory.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "nvidia/cuda_context.h"
#include "nvidia/nvidia_decoder_factory.h"
#include "nvidia/nvidia_encoder_factory.h"

namespace {

bool NvidiaIsRequired() {
  const char* value = std::getenv("LIVEKIT_REQUIRE_NVIDIA");
  return value != nullptr && std::strcmp(value, "0") != 0;
}

bool ContainsCodec(const std::vector<webrtc::SdpVideoFormat>& formats,
                   const std::string& name) {
  return std::any_of(formats.begin(), formats.end(),
                     [&](const auto& format) { return format.name == name; });
}

void PrintFormats(const char* factory_name,
                  const std::vector<webrtc::SdpVideoFormat>& formats) {
  std::cout << factory_name << " formats:";
  for (const auto& format : formats) {
    std::cout << ' ' << format.name;
  }
  std::cout << std::endl;
}

class CapturingEncodedImageCallback final
    : public webrtc::EncodedImageCallback {
 public:
  Result OnEncodedImage(
      const webrtc::EncodedImage& image,
      const webrtc::CodecSpecificInfo* /* codec_specific_info */) override {
    {
      std::lock_guard<std::mutex> lock(mutex_);
      encoded_size_ = image.GetEncodedData()->size();
      frame_type_ = image._frameType;
      completed_ = true;
    }
    condition_.notify_one();
    return Result(Result::OK);
  }

  void OnFrameDropped(uint32_t /* rtp_timestamp */,
                      int /* spatial_id */,
                      bool /* is_end_of_temporal_unit */) override {
    {
      std::lock_guard<std::mutex> lock(mutex_);
      dropped_ = true;
      completed_ = true;
    }
    condition_.notify_one();
  }

  bool WaitForResult() {
    std::unique_lock<std::mutex> lock(mutex_);
    return condition_.wait_for(lock, std::chrono::seconds(5),
                               [&] { return completed_; });
  }

  size_t encoded_size() const { return encoded_size_; }
  webrtc::VideoFrameType frame_type() const { return frame_type_; }
  bool dropped() const { return dropped_; }

 private:
  std::mutex mutex_;
  std::condition_variable condition_;
  bool completed_ = false;
  bool dropped_ = false;
  size_t encoded_size_ = 0;
  webrtc::VideoFrameType frame_type_ = webrtc::VideoFrameType::kVideoFrameDelta;
};

TEST(NvidiaHardwareTest, CudaContextLifecycle) {
  if (!livekit_ffi::CudaContext::IsAvailable()) {
    if (NvidiaIsRequired()) {
      FAIL() << "CUDA is required but unavailable";
    } else {
      GTEST_SKIP() << "CUDA is unavailable on this machine";
    }
  }

  auto* context = livekit_ffi::CudaContext::GetInstance();
  ASSERT_NE(context, nullptr);
  ASSERT_TRUE(context->Initialize());
  EXPECT_TRUE(context->IsInitialized());
  EXPECT_NE(context->GetContext(), nullptr);
  context->Shutdown();
  EXPECT_FALSE(context->IsInitialized());
}

TEST(NvidiaHardwareTest, FactoriesAdvertiseH264) {
  const bool nvenc_supported = webrtc::NvidiaVideoEncoderFactory::IsSupported();
  const bool nvdec_supported = webrtc::NvidiaVideoDecoderFactory::IsSupported();
  if (!nvenc_supported || !nvdec_supported) {
    if (NvidiaIsRequired()) {
      FAIL() << "NVENC and NVDEC are required but unavailable";
    } else {
      GTEST_SKIP() << "NVENC or NVDEC is unavailable on this machine";
    }
  }

  webrtc::NvidiaVideoEncoderFactory encoder_factory;
  webrtc::NvidiaVideoDecoderFactory decoder_factory;
  const auto encoder_formats = encoder_factory.GetSupportedFormats();
  const auto decoder_formats = decoder_factory.GetSupportedFormats();

  PrintFormats("NVENC", encoder_formats);
  PrintFormats("NVDEC", decoder_formats);
  EXPECT_TRUE(ContainsCodec(encoder_formats, "H264"));
  EXPECT_TRUE(ContainsCodec(decoder_formats, "H264"));
}

TEST(NvidiaHardwareTest, EncodesOneH264KeyFrame) {
  if (!webrtc::NvidiaVideoEncoderFactory::IsSupported()) {
    if (NvidiaIsRequired()) {
      FAIL() << "NVENC is required but unavailable";
    } else {
      GTEST_SKIP() << "NVENC is unavailable on this machine";
    }
  }

  webrtc::NvidiaVideoEncoderFactory factory;
  const auto formats = factory.GetSupportedFormats();
  const auto h264_format =
      std::find_if(formats.begin(), formats.end(),
                   [](const auto& format) { return format.name == "H264"; });
  ASSERT_NE(h264_format, formats.end());

  auto environment = webrtc::CreateEnvironment();
  auto encoder = factory.Create(environment, *h264_format);
  ASSERT_NE(encoder, nullptr);

  constexpr int kWidth = 320;
  constexpr int kHeight = 180;
  webrtc::VideoCodec codec;
  codec.codecType = webrtc::kVideoCodecH264;
  codec.width = kWidth;
  codec.height = kHeight;
  codec.maxFramerate = 30;
  codec.minBitrate = 100;
  codec.startBitrate = 500;
  codec.maxBitrate = 1000;
  codec.qpMax = 56;
  codec.numberOfSimulcastStreams = 1;
  codec.simulcastStream[0].width = kWidth;
  codec.simulcastStream[0].height = kHeight;
  codec.simulcastStream[0].minBitrate = codec.minBitrate;
  codec.simulcastStream[0].targetBitrate = codec.startBitrate;
  codec.simulcastStream[0].maxBitrate = codec.maxBitrate;
  codec.simulcastStream[0].maxFramerate = codec.maxFramerate;
  codec.simulcastStream[0].active = true;
  codec.SetScalabilityMode(webrtc::ScalabilityMode::kL1T1);
  codec.SetFrameDropEnabled(false);

  CapturingEncodedImageCallback callback;
  ASSERT_EQ(encoder->RegisterEncodeCompleteCallback(&callback), 0);
  ASSERT_EQ(
      encoder->InitEncode(
          &codec, webrtc::VideoEncoder::Settings(
                      webrtc::VideoEncoder::Capabilities(false), 4, 1440)),
      0);

  auto buffer = webrtc::I420Buffer::Create(kWidth, kHeight);
  for (int row = 0; row < kHeight; ++row) {
    std::memset(buffer->MutableDataY() + row * buffer->StrideY(), row, kWidth);
  }
  for (int row = 0; row < kHeight / 2; ++row) {
    std::memset(buffer->MutableDataU() + row * buffer->StrideU(), 96,
                kWidth / 2);
    std::memset(buffer->MutableDataV() + row * buffer->StrideV(), 160,
                kWidth / 2);
  }

  webrtc::VideoFrame frame = webrtc::VideoFrame::Builder()
                                 .set_video_frame_buffer(buffer)
                                 .set_rtp_timestamp(9000)
                                 .build();
  std::vector<webrtc::VideoFrameType> frame_types = {
      webrtc::VideoFrameType::kVideoFrameKey};

  ASSERT_EQ(encoder->Encode(frame, &frame_types), 0);
  ASSERT_TRUE(callback.WaitForResult());
  EXPECT_FALSE(callback.dropped());
  EXPECT_GT(callback.encoded_size(), 0u);
  EXPECT_EQ(callback.frame_type(), webrtc::VideoFrameType::kVideoFrameKey);
  EXPECT_EQ(encoder->Release(), 0);
}

}  // namespace
