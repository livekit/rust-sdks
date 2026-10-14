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
#include "rtc_base/bitstream_reader.h"
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

#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
// Follows uncompressed_header() in the AV1 spec up to the frame size.
std::optional<webrtc::Resolution> ParseAv1FrameSize(
    const Dav1dSequenceHeader& seq,
    int temporal_id,
    int spatial_id,
    std::span<const uint8_t> header,
    std::array<webrtc::Resolution, 8>& temporal_unit_refs) {
  webrtc::BitstreamReader reader(header);
  if (seq.reduced_still_picture_header || reader.ReadBit()) {
    return std::nullopt;
  }
  const int frame_type = reader.ReadBits(2);
  const bool show_frame = reader.ReadBit();
  if (show_frame && seq.decoder_model_info_present &&
      !seq.equal_picture_interval) {
    reader.ConsumeBits(seq.frame_presentation_delay_length);
  }
  if (!show_frame) {
    reader.ConsumeBits(1);
  }
  const bool intra = frame_type == DAV1D_FRAME_TYPE_KEY ||
                     frame_type == DAV1D_FRAME_TYPE_INTRA;
  const bool refresh_all = frame_type == DAV1D_FRAME_TYPE_SWITCH ||
                           (frame_type == DAV1D_FRAME_TYPE_KEY && show_frame);
  const bool error_resilient = refresh_all || reader.ReadBit();
  reader.ConsumeBits(1);
  const bool screen_content_tools = seq.screen_content_tools == DAV1D_ADAPTIVE
                                        ? reader.ReadBit()
                                        : seq.screen_content_tools == DAV1D_ON;
  if (screen_content_tools && seq.force_integer_mv == DAV1D_ADAPTIVE) {
    reader.ConsumeBits(1);
  }
  if (seq.frame_id_numbers_present) {
    reader.ConsumeBits(seq.frame_id_n_bits);
  }
  const bool size_override =
      frame_type == DAV1D_FRAME_TYPE_SWITCH || reader.ReadBit();
  const int order_hint_bits = seq.order_hint ? seq.order_hint_n_bits : 0;
  reader.ConsumeBits(order_hint_bits);
  if (!intra && !error_resilient) {
    reader.ConsumeBits(3);
  }
  if (seq.decoder_model_info_present && reader.ReadBit()) {
    for (int i = 0; i < seq.num_operating_points; ++i) {
      const auto& op = seq.operating_points[i];
      if (op.decoder_model_param_present &&
          (op.idc == 0 || (((op.idc >> temporal_id) & 1) &&
                           ((op.idc >> (spatial_id + 8)) & 1)))) {
        reader.ConsumeBits(seq.buffer_removal_delay_length);
      }
    }
  }
  const int refresh_frame_flags = refresh_all ? 0xff : reader.ReadBits(8);
  if ((!intra || refresh_frame_flags != 0xff) && error_resilient) {
    reader.ConsumeBits(8 * order_hint_bits);
  }
  std::optional<webrtc::Resolution> size;
  bool refs_from_order_hints = false;
  if (!intra) {
    refs_from_order_hints = seq.order_hint && reader.ReadBit();
    std::array<int, 7> ref_frame_idx;
    for (int& idx : ref_frame_idx) {
      idx = reader.ReadBits(3);
      if (seq.frame_id_numbers_present) {
        reader.ConsumeBits(seq.delta_frame_id_n_bits);
      }
    }
    if (size_override && !error_resilient) {
      for (int idx : ref_frame_idx) {
        if (reader.ReadBit()) {
          size = temporal_unit_refs[idx];
          break;
        }
      }
    }
  }
  if (!size && size_override) {
    size.emplace();
    size->width = reader.ReadBits(seq.width_n_bits) + 1;
    size->height = reader.ReadBits(seq.height_n_bits) + 1;
  } else if (!size) {
    size = webrtc::Resolution{seq.max_width, seq.max_height};
  }
  if (!reader.Ok() || refs_from_order_hints || size->width == 0) {
    return std::nullopt;
  }
  for (int i = 0; i < 8; ++i) {
    if (refresh_frame_flags & (1 << i)) {
      temporal_unit_refs[i] = *size;
    }
  }
  return size;
}

// The SDK does not negotiate the dependency descriptor, so libwebrtc gives AV1
// frames no size or spatial index. dav1d outputs only the highest spatial
// layer in each temporal unit.
std::optional<webrtc::Resolution> Av1FrameSize(
    const webrtc::EncodedImage& image,
    std::array<webrtc::Resolution, 4>& layer_sizes) {
  Dav1dSequenceHeader seq;
  const bool keyframe =
      image.IsKey() &&
      dav1d_parse_sequence_header(&seq, image.data(), image.size()) == 0;
  std::array<webrtc::Resolution, 8> temporal_unit_refs;
  int top_layer = -1;
  webrtc::BitstreamReader reader(
      std::span<const uint8_t>(image.data(), image.size()));
  while (reader.Ok() && reader.RemainingBitCount() > 0) {
    reader.ConsumeBits(1);
    const int type = reader.ReadBits(4);
    const bool has_extension = reader.ReadBit();
    const bool has_size = reader.ReadBit();
    reader.ConsumeBits(1);
    int temporal_id = 0;
    int spatial_id = 0;
    if (has_extension) {
      temporal_id = reader.ReadBits(3);
      spatial_id = reader.ReadBits(2);
      reader.ConsumeBits(3);
    }
    const uint64_t size =
        has_size ? reader.ReadLeb128() : reader.RemainingBitCount() / 8;
    if (!reader.Ok() ||
        size > static_cast<uint64_t>(reader.RemainingBitCount() / 8)) {
      break;
    }
    constexpr int kObuFrameHeader = 3;
    constexpr int kObuFrame = 6;
    if (type == kObuFrameHeader || type == kObuFrame) {
      top_layer = std::max(top_layer, spatial_id);
      if (keyframe) {
        const size_t offset = image.size() - reader.RemainingBitCount() / 8;
        if (const auto layer_size = ParseAv1FrameSize(
                seq, temporal_id, spatial_id,
                std::span<const uint8_t>(image.data() + offset, size),
                temporal_unit_refs)) {
          layer_sizes[spatial_id] = *layer_size;
        }
      }
    }
    reader.ConsumeBits(static_cast<int>(size * 8));
  }
  if (top_layer >= 0 && layer_sizes[top_layer] != webrtc::Resolution()) {
    return layer_sizes[top_layer];
  }
  if (keyframe) {
    return webrtc::Resolution{seq.max_width, seq.max_height};
  }
  return std::nullopt;
}
#endif

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
  if (codec_type_ == webrtc::kVideoCodecAV1) {
    if (const auto size = Av1FrameSize(input_image, av1_layer_sizes_)) {
      width = size->width;
      height = size->height;
    }
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
  av1_layer_sizes_ = {};
  return WEBRTC_VIDEO_CODEC_OK;
}

webrtc::VideoDecoder::DecoderInfo NullVideoDecoder::GetDecoderInfo() const {
  DecoderInfo info;
  info.implementation_name = "NullVideoDecoder";
  return info;
}

}  // namespace livekit_ffi
