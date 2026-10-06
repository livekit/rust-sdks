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

#include "livekit/null_video_decoder.h"

#include <algorithm>
#include <optional>
#include <span>

#include "api/make_ref_counted.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/vp9_uncompressed_header_parser.h"

#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
#include "third_party/dav1d/libdav1d/include/dav1d/dav1d.h"
#endif

namespace livekit_ffi {

namespace {

// Carries only a size. Nothing after the decoder reads pixels unless a sink
// converts, and each conversion gets its own buffer.
class BlackFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  BlackFrameBuffer(int width, int height) : width_(width), height_(height) {}

  Type type() const override { return Type::kNative; }
  int width() const override { return width_; }
  int height() const override { return height_; }

  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override {
    auto buffer = webrtc::I420Buffer::Create(width_, height_);
    webrtc::I420Buffer::SetBlack(buffer.get());
    return buffer;
  }

 private:
  const int width_;
  const int height_;
};

}  // namespace

bool NullVideoDecoder::Configure(const Settings& settings) {
  codec_type_ = settings.codec_type();
  return true;
}

int32_t NullVideoDecoder::Decode(const webrtc::EncodedImage& input_image,
                                 int64_t /* render_time_ms */) {
  if (!callback_) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  int width = static_cast<int>(input_image._encodedWidth);
  int height = static_cast<int>(input_image._encodedHeight);
  if (codec_type_ == webrtc::kVideoCodecVP9 && input_image.IsKey()) {
    // A keyframe superframe keeps its base layer's encoded size, while libvpx
    // outputs the top spatial layer, which ends the buffer.
    const size_t top_layer_size = std::min(
        input_image
            .SpatialLayerFrameSize(input_image.SpatialIndex().value_or(0))
            .value_or(input_image.size()),
        input_image.size());
    const auto header =
        webrtc::ParseUncompressedVp9Header(std::span<const uint8_t>(
            input_image.data() + input_image.size() - top_layer_size,
            top_layer_size));
    if (header && header->frame_width > 0) {
      width = header->frame_width;
      height = header->frame_height;
    }
  }
#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
  Dav1dSequenceHeader sequence_header;
  if (width == 0 && codec_type_ == webrtc::kVideoCodecAV1 &&
      input_image.IsKey() &&
      dav1d_parse_sequence_header(&sequence_header, input_image.data(),
                                  input_image.size()) == 0) {
    width = sequence_header.max_width;
    height = sequence_header.max_height;
  }
#endif
  if (width > 0 && height > 0 &&
      (!buffer_ || buffer_->width() != width || buffer_->height() != height)) {
    buffer_ = webrtc::make_ref_counted<BlackFrameBuffer>(width, height);
  }
  if (!buffer_) {
    // An error makes the receiver request a keyframe, which carries a size.
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // The receiver pairs this frame with its metadata by RTP timestamp.
  webrtc::VideoFrame frame = webrtc::VideoFrame::Builder()
                                 .set_video_frame_buffer(buffer_)
                                 .set_rtp_timestamp(input_image.RtpTimestamp())
                                 .build();
  callback_->Decoded(frame, std::nullopt, std::nullopt);
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t NullVideoDecoder::RegisterDecodeCompleteCallback(
    webrtc::DecodedImageCallback* callback) {
  callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t NullVideoDecoder::Release() {
  callback_ = nullptr;
  buffer_ = nullptr;
  return WEBRTC_VIDEO_CODEC_OK;
}

webrtc::VideoDecoder::DecoderInfo NullVideoDecoder::GetDecoderInfo() const {
  DecoderInfo info;
  info.implementation_name = "NullVideoDecoder";
  return info;
}

}  // namespace livekit_ffi
