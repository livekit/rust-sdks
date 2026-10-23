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

#include "livekit/apple_av1_decoder_factory.h"

#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <VideoToolbox/VideoToolbox.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>
#include <optional>
#include <vector>

#include "api/environment/environment.h"
#include "api/scoped_refptr.h"
#include "api/video/encoded_image.h"
#include "api/video/video_frame.h"
#include "api/video/video_frame_buffer.h"
#include "api/video_codecs/av1_profile.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_decoder.h"
#include "absl/strings/match.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/codecs/av1/av1_svc_config.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "sdk/objc/components/video_frame_buffer/RTCCVPixelBuffer.h"
#include "sdk/objc/native/api/video_frame_buffer.h"

#ifndef kCMVideoCodecType_AV1
#define kCMVideoCodecType_AV1 'av01'
#endif

namespace livekit_ffi {
namespace {

constexpr uint8_t kObuTypeSequenceHeader = 1;
constexpr uint8_t kObuTypeTemporalDelimiter = 2;
constexpr uint8_t kObuTypePadding = 15;
constexpr char kImplementationName[] = "VideoToolbox";

struct Obu {
  uint8_t type = 0;
  const uint8_t* data = nullptr;
  size_t size = 0;
  const uint8_t* payload = nullptr;
  size_t payload_size = 0;
};

struct Av1SequenceInfo {
  int width = 0;
  int height = 0;
  uint8_t profile = 0;
  uint8_t level = 0;
  uint8_t tier = 0;
  bool high_bitdepth = false;
  bool twelve_bit = false;
  bool monochrome = false;
  bool chroma_subsampling_x = true;
  bool chroma_subsampling_y = true;
  uint8_t chroma_sample_position = 0;
};

struct DecodeContext {
  webrtc::DecodedImageCallback* callback = nullptr;
  uint32_t rtp_timestamp = 0;
  int64_t ntp_time_ms = 0;
  const webrtc::ColorSpace* color_space = nullptr;
  bool decoded = false;
  OSStatus status = noErr;
};

class BitReader {
 public:
  BitReader(const uint8_t* data, size_t size) : data_(data), size_(size) {}

  bool ReadBits(size_t count, uint32_t* value) {
    if (count > 32 || bit_offset_ + count > size_ * 8) {
      return false;
    }
    uint32_t out = 0;
    for (size_t i = 0; i < count; ++i) {
      const size_t offset = bit_offset_++;
      const uint8_t byte = data_[offset / 8];
      out = (out << 1) | ((byte >> (7 - (offset % 8))) & 1);
    }
    *value = out;
    return true;
  }

  bool ReadBit(bool* value) {
    uint32_t bit = 0;
    if (!ReadBits(1, &bit)) {
      return false;
    }
    *value = bit != 0;
    return true;
  }

  bool SkipBits(size_t count) {
    uint32_t ignored = 0;
    while (count > 0) {
      const size_t step = std::min<size_t>(count, 32);
      if (!ReadBits(step, &ignored)) {
        return false;
      }
      count -= step;
    }
    return true;
  }

 private:
  const uint8_t* data_;
  size_t size_;
  size_t bit_offset_ = 0;
};

bool ReadLeb128(const uint8_t* data,
                size_t size,
                size_t* bytes_read,
                size_t* value) {
  size_t out = 0;
  for (size_t i = 0; i < std::min<size_t>(size, 8); ++i) {
    const uint8_t byte = data[i];
    out |= static_cast<size_t>(byte & 0x7f) << (i * 7);
    if ((byte & 0x80) == 0) {
      *bytes_read = i + 1;
      *value = out;
      return true;
    }
  }
  return false;
}

std::optional<Obu> ReadObu(const uint8_t* data, size_t size, size_t* offset) {
  if (*offset >= size) {
    return std::nullopt;
  }

  const size_t obu_start = *offset;
  const uint8_t header = data[(*offset)++];
  const bool extension_flag = (header & 0x04) != 0;
  const bool size_present = (header & 0x02) != 0;
  const uint8_t type = (header >> 3) & 0x0f;

  if (extension_flag) {
    if (*offset >= size) {
      return std::nullopt;
    }
    ++(*offset);
  }

  size_t payload_size = size - *offset;
  if (size_present) {
    size_t leb_size = 0;
    if (!ReadLeb128(data + *offset, size - *offset, &leb_size, &payload_size)) {
      return std::nullopt;
    }
    *offset += leb_size;
  }

  if (payload_size > size - *offset) {
    return std::nullopt;
  }

  Obu obu;
  obu.type = type;
  obu.data = data + obu_start;
  obu.payload = data + *offset;
  obu.payload_size = payload_size;
  *offset += payload_size;
  obu.size = *offset - obu_start;
  return obu;
}

std::optional<Obu> FindSequenceHeader(const uint8_t* data, size_t size) {
  size_t offset = 0;
  while (offset < size) {
    std::optional<Obu> obu = ReadObu(data, size, &offset);
    if (!obu) {
      return std::nullopt;
    }
    if (obu->type == kObuTypeSequenceHeader) {
      return obu;
    }
  }
  return std::nullopt;
}

bool SkipTimingInfo(BitReader* bits) {
  return bits->SkipBits(32) && bits->SkipBits(32) && bits->SkipBits(1);
}

bool SkipDecoderModelInfo(BitReader* bits) {
  uint32_t buffer_delay_length_minus_1 = 0;
  return bits->ReadBits(5, &buffer_delay_length_minus_1) &&
         bits->SkipBits(32) && bits->SkipBits(5) && bits->SkipBits(5);
}

bool SkipOperatingParametersInfo(BitReader* bits,
                                 uint32_t buffer_delay_length_minus_1) {
  const size_t delay_bits = buffer_delay_length_minus_1 + 1;
  return bits->SkipBits(delay_bits) && bits->SkipBits(delay_bits) &&
         bits->SkipBits(1);
}

bool ParseSequenceHeader(const Obu& sequence_header, Av1SequenceInfo* info) {
  BitReader bits(sequence_header.payload, sequence_header.payload_size);

  uint32_t value = 0;
  bool flag = false;
  bool reduced_still_picture_header = false;

  if (!bits.ReadBits(3, &value)) {
    return false;
  }
  info->profile = value;

  if (!bits.ReadBit(&flag) ||
      !bits.ReadBit(&reduced_still_picture_header)) {
    return false;
  }

  uint32_t operating_points_cnt_minus_1 = 0;
  bool decoder_model_info_present = false;
  uint32_t buffer_delay_length_minus_1 = 0;
  bool initial_display_delay_present = false;

  if (reduced_still_picture_header) {
    if (!bits.ReadBits(5, &value)) {
      return false;
    }
    info->level = value;
  } else {
    bool timing_info_present = false;
    if (!bits.ReadBit(&timing_info_present)) {
      return false;
    }
    if (timing_info_present) {
      bool decoder_model_info_present_for_timing = false;
      if (!SkipTimingInfo(&bits) ||
          !bits.ReadBit(&decoder_model_info_present_for_timing)) {
        return false;
      }
      decoder_model_info_present = decoder_model_info_present_for_timing;
      if (decoder_model_info_present) {
        const size_t before_decoder_model = 0;
        (void)before_decoder_model;
        if (!bits.ReadBits(5, &buffer_delay_length_minus_1) ||
            !bits.SkipBits(32) || !bits.SkipBits(5) || !bits.SkipBits(5)) {
          return false;
        }
      }
    }
    if (!bits.ReadBit(&initial_display_delay_present) ||
        !bits.ReadBits(5, &operating_points_cnt_minus_1)) {
      return false;
    }

    for (uint32_t i = 0; i <= operating_points_cnt_minus_1; ++i) {
      if (!bits.SkipBits(12) || !bits.ReadBits(5, &value)) {
        return false;
      }
      if (i == 0) {
        info->level = value;
      }
      if (value > 7) {
        bool tier = false;
        if (!bits.ReadBit(&tier)) {
          return false;
        }
        if (i == 0) {
          info->tier = tier ? 1 : 0;
        }
      }
      if (decoder_model_info_present) {
        bool decoder_model_present = false;
        if (!bits.ReadBit(&decoder_model_present)) {
          return false;
        }
        if (decoder_model_present &&
            !SkipOperatingParametersInfo(&bits, buffer_delay_length_minus_1)) {
          return false;
        }
      }
      if (initial_display_delay_present) {
        bool initial_display_delay_present_for_op = false;
        if (!bits.ReadBit(&initial_display_delay_present_for_op)) {
          return false;
        }
        if (initial_display_delay_present_for_op && !bits.SkipBits(4)) {
          return false;
        }
      }
    }
  }

  uint32_t frame_width_bits_minus_1 = 0;
  uint32_t frame_height_bits_minus_1 = 0;
  uint32_t max_frame_width_minus_1 = 0;
  uint32_t max_frame_height_minus_1 = 0;
  if (!bits.ReadBits(4, &frame_width_bits_minus_1) ||
      !bits.ReadBits(4, &frame_height_bits_minus_1) ||
      !bits.ReadBits(frame_width_bits_minus_1 + 1, &max_frame_width_minus_1) ||
      !bits.ReadBits(frame_height_bits_minus_1 + 1, &max_frame_height_minus_1)) {
    return false;
  }
  info->width = static_cast<int>(max_frame_width_minus_1 + 1);
  info->height = static_cast<int>(max_frame_height_minus_1 + 1);

  if (!reduced_still_picture_header) {
    bool frame_id_numbers_present = false;
    if (!bits.ReadBit(&frame_id_numbers_present)) {
      return false;
    }
    if (frame_id_numbers_present && !bits.SkipBits(7)) {
      return false;
    }
  }

  if (!bits.SkipBits(3)) {
    return false;
  }
  if (!reduced_still_picture_header && !bits.SkipBits(11)) {
    return false;
  }
  if (!bits.SkipBits(1)) {
    return false;
  }

  if (!bits.ReadBit(&info->high_bitdepth)) {
    return false;
  }
  if (info->profile == 2 && info->high_bitdepth) {
    if (!bits.ReadBit(&info->twelve_bit)) {
      return false;
    }
  }
  if (info->profile != 1) {
    if (!bits.ReadBit(&info->monochrome)) {
      return false;
    }
  }

  bool color_description_present = false;
  uint32_t color_primaries = 2;
  uint32_t transfer_characteristics = 2;
  uint32_t matrix_coefficients = 2;
  if (!bits.ReadBit(&color_description_present)) {
    return false;
  }
  if (color_description_present) {
    if (!bits.ReadBits(8, &color_primaries) ||
        !bits.ReadBits(8, &transfer_characteristics) ||
        !bits.ReadBits(8, &matrix_coefficients)) {
      return false;
    }
  }

  if (!bits.SkipBits(1)) {
    return false;
  }

  if (info->monochrome) {
    info->chroma_subsampling_x = true;
    info->chroma_subsampling_y = true;
    return true;
  }

  if (color_primaries == 1 && transfer_characteristics == 13 &&
      matrix_coefficients == 0) {
    info->chroma_subsampling_x = false;
    info->chroma_subsampling_y = false;
  } else if (info->profile == 0) {
    info->chroma_subsampling_x = true;
    info->chroma_subsampling_y = true;
  } else if (info->profile == 1) {
    info->chroma_subsampling_x = false;
    info->chroma_subsampling_y = false;
  } else {
    if (!bits.ReadBit(&info->chroma_subsampling_x) ||
        !bits.ReadBit(&info->chroma_subsampling_y)) {
      return false;
    }
  }

  if (info->chroma_subsampling_x && info->chroma_subsampling_y) {
    if (!bits.ReadBits(2, &value)) {
      return false;
    }
    info->chroma_sample_position = value;
  }

  return true;
}

std::vector<uint8_t> BuildAv1C(const Av1SequenceInfo& info,
                               const Obu& sequence_header) {
  std::vector<uint8_t> av1c;
  av1c.reserve(4 + sequence_header.size);
  av1c.push_back(0x80 | 0x01);
  av1c.push_back((info.profile << 5) | (info.level & 0x1f));
  av1c.push_back((info.tier << 7) | (info.high_bitdepth ? 0x40 : 0) |
                 (info.twelve_bit ? 0x20 : 0) |
                 (info.monochrome ? 0x10 : 0) |
                 (info.chroma_subsampling_x ? 0x08 : 0) |
                 (info.chroma_subsampling_y ? 0x04 : 0) |
                 (info.chroma_sample_position & 0x03));
  av1c.push_back(0);
  av1c.insert(av1c.end(), sequence_header.data,
              sequence_header.data + sequence_header.size);
  return av1c;
}

CFDictionaryRef CreateFormatDescriptionExtensions(const std::vector<uint8_t>& av1c) {
  CFDataRef av1c_data =
      CFDataCreate(kCFAllocatorDefault, av1c.data(), av1c.size());
  if (av1c_data == nullptr) {
    return nullptr;
  }

  const void* atom_keys[] = {CFSTR("av1C")};
  const void* atom_values[] = {av1c_data};
  CFDictionaryRef atoms = CFDictionaryCreate(
      kCFAllocatorDefault, atom_keys, atom_values, 1,
      &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  CFRelease(av1c_data);
  if (atoms == nullptr) {
    return nullptr;
  }

  const void* extension_keys[] = {kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms};
  const void* extension_values[] = {atoms};
  CFDictionaryRef extensions = CFDictionaryCreate(
      kCFAllocatorDefault, extension_keys, extension_values, 1,
      &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
  CFRelease(atoms);
  return extensions;
}

void DecompressionOutputCallback(void* decompression_output_ref_con,
                                 void* source_frame_ref_con,
                                 OSStatus status,
                                 VTDecodeInfoFlags,
                                 CVImageBufferRef image_buffer,
                                 CMTime,
                                 CMTime) {
  DecodeContext* context = static_cast<DecodeContext*>(source_frame_ref_con);
  if (context == nullptr) {
    return;
  }

  context->status = status;
  if (status != noErr || image_buffer == nullptr || context->callback == nullptr) {
    return;
  }

  CVPixelBufferRetain(image_buffer);
  RTC_OBJC_TYPE(RTCCVPixelBuffer)* pixel_buffer =
      [[RTC_OBJC_TYPE(RTCCVPixelBuffer) alloc]
          initWithPixelBuffer:static_cast<CVPixelBufferRef>(image_buffer)];
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> frame_buffer =
      webrtc::ObjCToNativeVideoFrameBuffer(pixel_buffer);
  [pixel_buffer release];
  CVPixelBufferRelease(image_buffer);

  webrtc::VideoFrame frame = webrtc::VideoFrame::Builder()
                                 .set_video_frame_buffer(frame_buffer)
                                 .set_timestamp_rtp(context->rtp_timestamp)
                                 .set_ntp_time_ms(context->ntp_time_ms)
                                 .set_color_space(context->color_space)
                                 .build();
  context->callback->Decoded(frame, std::nullopt, std::nullopt);
  context->decoded = true;
}

class AppleAv1Decoder : public webrtc::VideoDecoder {
 public:
  AppleAv1Decoder() = default;
  AppleAv1Decoder(const AppleAv1Decoder&) = delete;
  AppleAv1Decoder& operator=(const AppleAv1Decoder&) = delete;
  ~AppleAv1Decoder() override { Release(); }

  static bool IsSupported() {
    if (@available(macOS 13.0, *)) {
      return VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1);
    }
    return false;
  }

  bool Configure(const Settings&) override { return IsSupported(); }

  int32_t Decode(const webrtc::EncodedImage& input_image,
                 int64_t) override {
    if (callback_ == nullptr || input_image.data() == nullptr ||
        input_image.size() == 0) {
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    }

    std::optional<Obu> sequence_header =
        FindSequenceHeader(input_image.data(), input_image.size());
    if (sequence_header) {
      Av1SequenceInfo sequence_info;
      if (ParseSequenceHeader(*sequence_header, &sequence_info)) {
        if (!CreateSession(sequence_info, *sequence_header)) {
          return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
        }
      }
    }

    if (session_ == nullptr || format_description_ == nullptr) {
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }

    CMBlockBufferRef block = nullptr;
    OSStatus status = CMBlockBufferCreateWithMemoryBlock(
        kCFAllocatorDefault, nullptr, input_image.size(), kCFAllocatorDefault,
        nullptr, 0, input_image.size(), 0, &block);
    if (status != kCMBlockBufferNoErr) {
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    status = CMBlockBufferReplaceDataBytes(input_image.data(), block, 0,
                                           input_image.size());
    if (status != kCMBlockBufferNoErr) {
      CFRelease(block);
      return WEBRTC_VIDEO_CODEC_ERROR;
    }

    CMSampleTimingInfo timing = {
        .duration = kCMTimeInvalid,
        .presentationTimeStamp = kCMTimeInvalid,
        .decodeTimeStamp = kCMTimeInvalid,
    };
    const size_t sample_size = input_image.size();
    CMSampleBufferRef sample = nullptr;
    status = CMSampleBufferCreateReady(
        kCFAllocatorDefault, block, format_description_, 1, 1, &timing, 1,
        &sample_size, &sample);
    CFRelease(block);
    if (status != noErr) {
      RTC_LOG(LS_WARNING) << "CMSampleBufferCreateReady for AV1 failed: "
                          << status;
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }

    DecodeContext context;
    context.callback = callback_;
    context.rtp_timestamp = input_image.RtpTimestamp();
    context.ntp_time_ms = input_image.NtpTimeMs();
    context.color_space = input_image.ColorSpace();

    VTDecodeFrameFlags flags = kVTDecodeFrame_EnableAsynchronousDecompression;
    status = VTDecompressionSessionDecodeFrame(session_, sample, flags,
                                               &context, nullptr);
    CFRelease(sample);
    if (status != noErr) {
      RTC_LOG(LS_WARNING) << "VideoToolbox AV1 decode failed: " << status;
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }

    VTDecompressionSessionWaitForAsynchronousFrames(session_);
    if (context.status != noErr || !context.decoded) {
      RTC_LOG(LS_WARNING) << "VideoToolbox AV1 output failed: "
                          << context.status;
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }

    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback* callback) override {
    callback_ = callback;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Release() override {
    if (session_ != nullptr) {
      VTDecompressionSessionWaitForAsynchronousFrames(session_);
      VTDecompressionSessionInvalidate(session_);
      CFRelease(session_);
      session_ = nullptr;
    }
    if (format_description_ != nullptr) {
      CFRelease(format_description_);
      format_description_ = nullptr;
    }
    width_ = 0;
    height_ = 0;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  DecoderInfo GetDecoderInfo() const override {
    DecoderInfo info;
    info.implementation_name = kImplementationName;
    info.is_hardware_accelerated = true;
    return info;
  }

  const char* ImplementationName() const override {
    return kImplementationName;
  }

 private:
  bool CreateSession(const Av1SequenceInfo& sequence_info,
                     const Obu& sequence_header) {
    if (session_ != nullptr && format_description_ != nullptr &&
        width_ == sequence_info.width && height_ == sequence_info.height) {
      return true;
    }

    Release();

    std::vector<uint8_t> av1c = BuildAv1C(sequence_info, sequence_header);
    CFDictionaryRef extensions = CreateFormatDescriptionExtensions(av1c);
    if (extensions == nullptr) {
      return false;
    }

    OSStatus status = CMVideoFormatDescriptionCreate(
        kCFAllocatorDefault, kCMVideoCodecType_AV1, sequence_info.width,
        sequence_info.height, extensions, &format_description_);
    CFRelease(extensions);
    if (status != noErr) {
      RTC_LOG(LS_WARNING) << "CMVideoFormatDescriptionCreate for AV1 failed: "
                          << status;
      return false;
    }

    const void* destination_keys[] = {
        kCVPixelBufferPixelFormatTypeKey,
        kCVPixelBufferIOSurfacePropertiesKey,
    };
    const uint32_t pixel_format = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
    CFNumberRef pixel_format_number = CFNumberCreate(
        kCFAllocatorDefault, kCFNumberSInt32Type, &pixel_format);
    CFDictionaryRef io_surface_properties = CFDictionaryCreate(
        kCFAllocatorDefault, nullptr, nullptr, 0,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    const void* destination_values[] = {pixel_format_number,
                                        io_surface_properties};
    CFDictionaryRef destination_attributes = CFDictionaryCreate(
        kCFAllocatorDefault, destination_keys, destination_values, 2,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    CFRelease(pixel_format_number);
    CFRelease(io_surface_properties);

    VTDecompressionOutputCallbackRecord callback_record = {
        DecompressionOutputCallback, this};
    status = VTDecompressionSessionCreate(
        kCFAllocatorDefault, format_description_, nullptr,
        destination_attributes, &callback_record, &session_);
    CFRelease(destination_attributes);
    if (status != noErr) {
      RTC_LOG(LS_WARNING) << "VTDecompressionSessionCreate for AV1 failed: "
                          << status;
      CFRelease(format_description_);
      format_description_ = nullptr;
      return false;
    }

    width_ = sequence_info.width;
    height_ = sequence_info.height;
    RTC_LOG(LS_INFO) << "Using VideoToolbox HW decoder for AV1";
    return true;
  }

  webrtc::DecodedImageCallback* callback_ = nullptr;
  CMVideoFormatDescriptionRef format_description_ = nullptr;
  VTDecompressionSessionRef session_ = nullptr;
  int width_ = 0;
  int height_ = 0;
};

class AppleAv1DecoderFactory : public webrtc::VideoDecoderFactory {
 public:
  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    if (!AppleAv1Decoder::IsSupported()) {
      return {};
    }
    return {webrtc::SdpVideoFormat(
        webrtc::SdpVideoFormat::AV1Profile0(),
        webrtc::LibaomAv1EncoderSupportedScalabilityModes())};
  }

  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
                                 bool) const override {
    CodecSupport support;
    support.is_supported =
        AppleAv1Decoder::IsSupported() &&
        absl::EqualsIgnoreCase(format.name, webrtc::kAv1CodecName);
    support.is_power_efficient = support.is_supported;
    return support;
  }

  std::unique_ptr<webrtc::VideoDecoder> Create(
      const webrtc::Environment&,
      const webrtc::SdpVideoFormat& format) override {
    if (!AppleAv1Decoder::IsSupported() ||
        !absl::EqualsIgnoreCase(format.name, webrtc::kAv1CodecName)) {
      return nullptr;
    }
    return std::make_unique<AppleAv1Decoder>();
  }
};

}  // namespace

std::unique_ptr<webrtc::VideoDecoderFactory> CreateAppleAv1DecoderFactory() {
  return std::make_unique<AppleAv1DecoderFactory>();
}

}  // namespace livekit_ffi
