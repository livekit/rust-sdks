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

#include "livekit/passthrough_video_decoder.h"

#include "api/video/video_frame.h"
#include "av1_bitstream.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"

#ifdef LIVEKIT_TEST
#include <string_view>

#include "api/environment/environment_factory.h"
#include "webrtc-sys/src/passthrough_video_decoder.rs.h"
#endif

namespace livekit_ffi {

namespace {

// Identifies a decoder instance in the buffers it emits. A counter, not the
// decoder's address: a new decoder can be allocated where a destroyed one was,
// and the application must still see it as a new stream.
const void* NextStreamId() {
  static std::atomic<uintptr_t> next_id{1};
  return reinterpret_cast<const void*>(next_id.fetch_add(1));
}

}  // namespace

PassthroughVideoDecoder::PassthroughVideoDecoder(
    livekit::EncodedVideoCodec codec)
    : codec_(codec), stream_id_(NextStreamId()) {}

bool PassthroughVideoDecoder::Configure(const Settings& /* settings */) {
  return true;
}

int32_t PassthroughVideoDecoder::Decode(const webrtc::EncodedImage& input_image,
                                        int64_t /* render_time_ms */) {
  if (!callback_) {
    RTC_LOG(LS_ERROR) << "PassthroughVideoDecoder callback is not registered";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface> payload =
      input_image.GetEncodedData();
  if (!payload || input_image.size() == 0) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  // Share the received buffer when it holds exactly this access unit;
  // otherwise copy the used prefix.
  if (payload->size() != input_image.size()) {
    payload = webrtc::EncodedImageBuffer::Create(input_image.data(),
                                                 input_image.size());
  }

  const bool keyframe =
      input_image._frameType == webrtc::VideoFrameType::kVideoFrameKey;
  // libwebrtc reports a frame size on keyframes only (from the H264/H265 SPS)
  // and never for AV1, and does not reconfigure the decoder when the size
  // changes. Keep the last known size for delta frames.
  if (input_image._encodedWidth > 0 && input_image._encodedHeight > 0) {
    width_ = static_cast<int>(input_image._encodedWidth);
    height_ = static_cast<int>(input_image._encodedHeight);
  } else if (keyframe && codec_ == livekit::EncodedVideoCodec::kAV1) {
    int width = 0;
    int height = 0;
    if (livekit::av1::ParseSequenceHeaderFrameSize(
            payload->data(), payload->size(), &width, &height)) {
      width_ = width;
      height_ = height;
    }
  }

  auto buffer = webrtc::make_ref_counted<livekit::EncodedVideoFrameBuffer>(
      width_, height_, codec_,
      keyframe ? livekit::EncodedFrameType::kKey
               : livekit::EncodedFrameType::kDelta,
      std::move(payload), keyframe_request_flag_);
  buffer->set_stream_position(stream_id_, sequence_number_++);

  webrtc::VideoFrame frame = webrtc::VideoFrame::Builder()
                                 .set_video_frame_buffer(buffer)
                                 .set_rtp_timestamp(input_image.RtpTimestamp())
                                 .set_rotation(input_image.rotation_)
                                 .build();
  callback_->Decoded(frame, std::nullopt, std::nullopt);

  // A request raised through an emitted buffer asks the sender for a
  // keyframe. The frame itself is still delivered.
  return keyframe_request_flag_->exchange(false)
             ? WEBRTC_VIDEO_CODEC_OK_REQUEST_KEYFRAME
             : WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoDecoder::RegisterDecodeCompleteCallback(
    webrtc::DecodedImageCallback* callback) {
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t PassthroughVideoDecoder::Release() {
  callback_ = nullptr;
  return WEBRTC_VIDEO_CODEC_OK;
}

webrtc::VideoDecoder::DecoderInfo PassthroughVideoDecoder::GetDecoderInfo()
    const {
  DecoderInfo info;
  info.implementation_name = ImplementationName();
  info.is_hardware_accelerated = false;
  return info;
}

const char* PassthroughVideoDecoder::ImplementationName() const {
  return "LiveKit encoded passthrough";
}

PassthroughVideoDecoderFactory::PassthroughVideoDecoderFactory() = default;

std::vector<webrtc::SdpVideoFormat>
PassthroughVideoDecoderFactory::GetSupportedFormats() const {
  return supported_formats_;
}

std::unique_ptr<webrtc::VideoDecoder> PassthroughVideoDecoderFactory::Create(
    const webrtc::Environment& /* env */,
    const webrtc::SdpVideoFormat& /* format */) {
  return nullptr;
}

#ifdef LIVEKIT_TEST
namespace {

class CapturingCallback final : public webrtc::DecodedImageCallback {
 public:
  int32_t Decoded(webrtc::VideoFrame& frame) override {
    frames.push_back(frame);
    return WEBRTC_VIDEO_CODEC_OK;
  }
  std::vector<webrtc::VideoFrame> frames;
};

livekit::EncodedVideoCodec CodecForTest(std::string_view name) {
  if (name == "H265")
    return livekit::EncodedVideoCodec::kH265;
  if (name == "VP8")
    return livekit::EncodedVideoCodec::kVP8;
  if (name == "VP9")
    return livekit::EncodedVideoCodec::kVP9;
  if (name == "AV1")
    return livekit::EncodedVideoCodec::kAV1;
  return livekit::EncodedVideoCodec::kH264;
}

}  // namespace

rust::Vec<DecodeForTest> passthrough_decode_for_test(
    rust::Str codec,
    rust::Slice<const uint8_t> script,
    rust::Slice<const uint8_t> payload) {
  PassthroughVideoDecoder decoder(
      CodecForTest(std::string_view(codec.data(), codec.size())));
  decoder.Configure(webrtc::VideoDecoder::Settings());
  CapturingCallback callback;
  decoder.RegisterDecodeCompleteCallback(&callback);

  rust::Vec<DecodeForTest> results;
  uint32_t rtp_timestamp = 1000;
  for (uint8_t op : script) {
    if (op == kDecodeTestSetKeyframeFlag) {
      // The application raises the flag through a buffer it received.
      if (!callback.frames.empty()) {
        auto* buffer = livekit::EncodedVideoFrameBuffer::FromNative(
            callback.frames.back().video_frame_buffer().get());
        if (buffer)
          buffer->request_keyframe();
      }
      continue;
    }

    auto data =
        webrtc::EncodedImageBuffer::Create(payload.data(), payload.size());
    webrtc::EncodedImage image;
    image.SetEncodedData(data);
    image.SetRtpTimestamp(rtp_timestamp += 3000);
    const bool key = op != kDecodeTestDelta;
    image._frameType = key ? webrtc::VideoFrameType::kVideoFrameKey
                           : webrtc::VideoFrameType::kVideoFrameDelta;
    if (op == kDecodeTestKey64x64) {
      image._encodedWidth = 64;
      image._encodedHeight = 64;
    } else if (op == kDecodeTestKey128x72) {
      image._encodedWidth = 128;
      image._encodedHeight = 72;
    }

    const size_t before = callback.frames.size();
    DecodeForTest result{};
    result.return_code = decoder.Decode(image, /*render_time_ms=*/0);
    result.emitted = callback.frames.size() == before + 1;
    if (result.emitted) {
      const webrtc::VideoFrame& frame = callback.frames.back();
      auto* buffer = livekit::EncodedVideoFrameBuffer::FromNative(
          frame.video_frame_buffer().get());
      result.rtp_timestamp_matches = frame.rtp_timestamp() == rtp_timestamp;
      result.width = frame.width();
      result.height = frame.height();
      if (buffer) {
        result.shares_payload = buffer->payload_data() == data->data();
        result.sequence = buffer->sequence_number();
        result.keyframe =
            buffer->frame_type() == livekit::EncodedFrameType::kKey;
      }
    }
    results.push_back(result);
  }
  return results;
}

rust::String passthrough_decoder_info_for_test() {
  PassthroughVideoDecoder decoder(livekit::EncodedVideoCodec::kH264);
  auto info = decoder.GetDecoderInfo();
  return rust::String(info.implementation_name + "|" +
                      (info.is_hardware_accelerated ? "1" : "0"));
}

rust::Vec<size_t> passthrough_decoder_identities_for_test() {
  rust::Vec<size_t> ids;
  for (int i = 0; i < 2; ++i) {
    PassthroughVideoDecoder decoder(livekit::EncodedVideoCodec::kVP8);
    CapturingCallback callback;
    decoder.RegisterDecodeCompleteCallback(&callback);
    const uint8_t payload[] = {0x10, 0x02, 0x00};
    webrtc::EncodedImage image;
    image.SetEncodedData(
        webrtc::EncodedImageBuffer::Create(payload, sizeof(payload)));
    image._frameType = webrtc::VideoFrameType::kVideoFrameKey;
    image._encodedWidth = 64;
    image._encodedHeight = 64;
    decoder.Decode(image, 0);
    size_t id = 0;
    if (!callback.frames.empty()) {
      auto* buffer = livekit::EncodedVideoFrameBuffer::FromNative(
          callback.frames.back().video_frame_buffer().get());
      if (buffer)
        id = reinterpret_cast<size_t>(buffer->stream_id());
    }
    ids.push_back(id);
  }
  return ids;
}
#endif

}  // namespace livekit_ffi
