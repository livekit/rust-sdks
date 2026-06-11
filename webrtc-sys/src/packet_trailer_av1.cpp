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

#include "livekit/packet_trailer_av1.h"

#include <algorithm>
#include <cctype>
#include <cstddef>
#include <cstring>
#include <string>

namespace livekit_ffi {
namespace av1 {

namespace {

constexpr uint8_t kAv1ObuSizePresentBit = 0b0000'0010;
constexpr uint8_t kAv1ObuExtensionFlag = 0b0000'0100;
constexpr uint8_t kAv1ObuTypeMask = 0b0111'1000;
constexpr uint8_t kAv1ObuForbiddenBit = 0b1000'0000;
constexpr uint8_t kAv1ObuTypeSequenceHeader = 1;
constexpr uint8_t kAv1ObuTypeTemporalDelimiter = 2;
constexpr uint8_t kAv1ObuTypeMetadata = 5;
constexpr uint8_t kAv1ObuTypeFrame = 6;
constexpr uint64_t kAv1MetadataTypeLiveKitPacketTrailer = 31;

// Magic bytes identifying a wrapped encrypted payload: "LKEP" (LiveKit
// Encrypted Payload). They follow the synthetic frame header byte inside
// the wrapper frame OBU so the wrapper cannot be mistaken for a real
// frame OBU when unwrapping.
constexpr uint8_t kEncryptedPayloadMagic[4] = {'L', 'K', 'E', 'P'};

// Fallback sequence_header_obu() payload prepended to wrapped keyframes
// when the frame's real sequence header is unavailable, so SFUs that gate
// keyframe detection on a leading sequence header OBU keep working.
// Decoders never see it (it is removed by UnwrapEncryptedPayload before
// decryption). Content: profile 0, level 2.0, no timing info, 4:2:0, no
// optional coding tools, and a deliberately absurd 16x16 max frame size so
// any decoder that does end up parsing it (a misconfigured receive path)
// fails in an obviously-synthetic way instead of mimicking a real stream.
constexpr uint8_t kSyntheticSequenceHeaderPayload[] = {
    0x00, 0x00, 0x00, 0x01, 0x9F, 0xFA, 0x03, 0x00, 0x10};

// First payload byte of the wrapper frame OBU, mimicking the start of
// uncompressed_header(): show_existing_frame=0, frame_type=KEY_FRAME for
// keyframes (SFU keyframe detection checks these bits on the first frame
// OBU after the sequence header) or frame_type=INTER_FRAME for delta
// frames, show_frame=1.
constexpr uint8_t kSyntheticKeyFrameHeaderByte = 0x10;
constexpr uint8_t kSyntheticInterFrameHeaderByte = 0x30;

void WriteLeb128(uint64_t value, std::vector<uint8_t>& out) {
  while (value >= 0x80) {
    out.push_back(static_cast<uint8_t>((value & 0x7F) | 0x80));
    value >>= 7;
  }
  out.push_back(static_cast<uint8_t>(value));
}

bool ReadLeb128(webrtc::ArrayView<const uint8_t> data,
                size_t& pos,
                uint64_t& value) {
  value = 0;
  int shift = 0;
  for (int bytes = 0; bytes < 8; ++bytes) {
    if (pos >= data.size()) {
      return false;
    }
    uint8_t byte = data[pos++];
    value |= static_cast<uint64_t>(byte & 0x7F) << shift;
    if ((byte & 0x80) == 0) {
      return true;
    }
    shift += 7;
  }
  return false;
}

std::vector<uint8_t> BuildMetadataObu(
    webrtc::ArrayView<const uint8_t> trailer) {
  std::vector<uint8_t> metadata_payload;
  WriteLeb128(kAv1MetadataTypeLiveKitPacketTrailer, metadata_payload);
  metadata_payload.insert(metadata_payload.end(), trailer.begin(), trailer.end());

  std::vector<uint8_t> obu;
  obu.reserve(1 + 8 + metadata_payload.size());
  obu.push_back(static_cast<uint8_t>((kAv1ObuTypeMetadata << 3) |
                                    kAv1ObuSizePresentBit));
  WriteLeb128(metadata_payload.size(), obu);
  obu.insert(obu.end(), metadata_payload.begin(), metadata_payload.end());
  return obu;
}

size_t FindMetadataInsertOffset(webrtc::ArrayView<const uint8_t> data) {
  size_t pos = 0;
  size_t insert_offset = 0;

  while (pos < data.size()) {
    const size_t obu_start = pos;
    uint8_t obu_header = data[pos++];
    if ((obu_header & 0x80) != 0) {
      return 0;
    }

    const uint8_t obu_type = (obu_header & kAv1ObuTypeMask) >> 3;
    if ((obu_header & kAv1ObuExtensionFlag) != 0) {
      if (pos >= data.size()) {
        return 0;
      }
      ++pos;
    }

    size_t payload_size = data.size() - pos;
    if ((obu_header & kAv1ObuSizePresentBit) != 0) {
      uint64_t leb_payload_size = 0;
      if (!ReadLeb128(data, pos, leb_payload_size) ||
          leb_payload_size > data.size() - pos) {
        return 0;
      }
      payload_size = static_cast<size_t>(leb_payload_size);
    }

    const size_t obu_end = pos + payload_size;
    if (obu_type == kAv1ObuTypeTemporalDelimiter) {
      pos = obu_end;
      continue;
    }

    if (obu_type != kAv1ObuTypeSequenceHeader) {
      break;
    }

    insert_offset = obu_end;
    pos = obu_end;

    if ((data[obu_start] & kAv1ObuSizePresentBit) == 0) {
      break;
    }
  }

  return insert_offset;
}

}  // namespace

bool IsAv1Frame(const webrtc::TransformableFrameInterface& frame) {
  std::string mime_type = frame.GetMimeType();
  std::transform(mime_type.begin(), mime_type.end(), mime_type.begin(),
                 [](unsigned char c) {
                   return static_cast<char>(std::tolower(c));
                 });
  return mime_type.find("av1") != std::string::npos;
}

std::vector<uint8_t> InsertTrailerObu(
    webrtc::ArrayView<const uint8_t> data,
    webrtc::ArrayView<const uint8_t> trailer) {
  std::vector<uint8_t> obu = BuildMetadataObu(trailer);
  if (data.empty()) {
    return obu;
  }

  const size_t insert_offset = FindMetadataInsertOffset(data);
  std::vector<uint8_t> result;
  result.reserve(data.size() + obu.size());
  result.insert(result.end(), data.begin(), data.begin() + insert_offset);
  result.insert(result.end(), obu.begin(), obu.end());
  result.insert(result.end(), data.begin() + insert_offset, data.end());
  return result;
}

std::optional<PacketTrailerMetadata> ExtractTrailer(
    webrtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data) {
  std::vector<uint8_t> stripped_data;
  stripped_data.reserve(data.size());
  size_t pos = 0;

  while (pos < data.size()) {
    const size_t obu_start = pos;
    uint8_t obu_header = data[pos++];
    if ((obu_header & 0x80) != 0) {
      out_data.assign(data.begin(), data.end());
      return std::nullopt;
    }

    const uint8_t obu_type = (obu_header & kAv1ObuTypeMask) >> 3;
    if ((obu_header & kAv1ObuExtensionFlag) != 0) {
      if (pos >= data.size()) {
        out_data.assign(data.begin(), data.end());
        return std::nullopt;
      }
      ++pos;
    }

    size_t payload_size = data.size() - pos;
    if ((obu_header & kAv1ObuSizePresentBit) != 0) {
      uint64_t leb_payload_size = 0;
      if (!ReadLeb128(data, pos, leb_payload_size) ||
          leb_payload_size > data.size() - pos) {
        out_data.assign(data.begin(), data.end());
        return std::nullopt;
      }
      payload_size = static_cast<size_t>(leb_payload_size);
    }

    const size_t payload_start = pos;
    const size_t obu_end = payload_start + payload_size;

    if (obu_type == kAv1ObuTypeMetadata) {
      auto metadata_payload = data.subview(payload_start, obu_end - payload_start);
      size_t metadata_pos = 0;
      uint64_t metadata_type = 0;
      if (ReadLeb128(metadata_payload, metadata_pos, metadata_type) &&
          metadata_type == kAv1MetadataTypeLiveKitPacketTrailer &&
          metadata_pos <= metadata_payload.size()) {
        auto trailer_payload = metadata_payload.subview(
            metadata_pos, metadata_payload.size() - metadata_pos);
        if (auto meta = ParseTrailerPayload(trailer_payload)) {
          stripped_data.insert(stripped_data.end(), data.begin() + obu_end,
                               data.end());
          out_data = std::move(stripped_data);
          return meta;
        }
      }
    }

    stripped_data.insert(stripped_data.end(), data.begin() + obu_start,
                         data.begin() + obu_end);

    pos = obu_end;
  }

  out_data.assign(data.begin(), data.end());
  return std::nullopt;
}

std::optional<std::vector<uint8_t>> ExtractSequenceHeaderObu(
    webrtc::ArrayView<const uint8_t> data) {
  size_t pos = 0;

  while (pos < data.size()) {
    uint8_t obu_header = data[pos++];
    if ((obu_header & kAv1ObuForbiddenBit) != 0) {
      return std::nullopt;
    }

    const uint8_t obu_type = (obu_header & kAv1ObuTypeMask) >> 3;
    size_t extension_size = 0;
    uint8_t extension_header = 0;
    if ((obu_header & kAv1ObuExtensionFlag) != 0) {
      if (pos >= data.size()) {
        return std::nullopt;
      }
      extension_header = data[pos++];
      extension_size = 1;
    }

    size_t payload_size = data.size() - pos;
    if ((obu_header & kAv1ObuSizePresentBit) != 0) {
      uint64_t leb_payload_size = 0;
      if (!ReadLeb128(data, pos, leb_payload_size) ||
          leb_payload_size > data.size() - pos) {
        return std::nullopt;
      }
      payload_size = static_cast<size_t>(leb_payload_size);
    }

    if (obu_type == kAv1ObuTypeSequenceHeader) {
      // Re-emit with an explicit size field so the resulting OBU survives
      // RTP packetization byte-exactly regardless of the source encoding.
      std::vector<uint8_t> obu;
      obu.reserve(1 + extension_size + 8 + payload_size);
      obu.push_back(obu_header | kAv1ObuSizePresentBit);
      if (extension_size != 0) {
        obu.push_back(extension_header);
      }
      WriteLeb128(payload_size, obu);
      obu.insert(obu.end(), data.begin() + pos, data.begin() + pos + payload_size);
      return obu;
    }

    pos += payload_size;
  }

  return std::nullopt;
}

std::vector<uint8_t> WrapEncryptedPayload(
    webrtc::ArrayView<const uint8_t> data,
    bool is_keyframe,
    webrtc::ArrayView<const uint8_t> sequence_header_obu) {
  const size_t frame_obu_payload_size =
      1 + sizeof(kEncryptedPayloadMagic) + data.size();

  std::vector<uint8_t> out;
  out.reserve(2 + sizeof(kSyntheticSequenceHeaderPayload) +
              sequence_header_obu.size() + 1 + 8 + frame_obu_payload_size);

  if (is_keyframe) {
    if (!sequence_header_obu.empty()) {
      out.insert(out.end(), sequence_header_obu.begin(),
                 sequence_header_obu.end());
    } else {
      out.push_back((kAv1ObuTypeSequenceHeader << 3) | kAv1ObuSizePresentBit);
      WriteLeb128(sizeof(kSyntheticSequenceHeaderPayload), out);
      out.insert(out.end(), std::begin(kSyntheticSequenceHeaderPayload),
                 std::end(kSyntheticSequenceHeaderPayload));
    }
  }

  out.push_back((kAv1ObuTypeFrame << 3) | kAv1ObuSizePresentBit);
  WriteLeb128(frame_obu_payload_size, out);
  out.push_back(is_keyframe ? kSyntheticKeyFrameHeaderByte
                            : kSyntheticInterFrameHeaderByte);
  out.insert(out.end(), std::begin(kEncryptedPayloadMagic),
             std::end(kEncryptedPayloadMagic));
  out.insert(out.end(), data.begin(), data.end());
  return out;
}

std::optional<std::vector<uint8_t>> UnwrapEncryptedPayload(
    webrtc::ArrayView<const uint8_t> data) {
  size_t pos = 0;

  while (pos < data.size()) {
    uint8_t obu_header = data[pos++];
    if ((obu_header & kAv1ObuForbiddenBit) != 0) {
      return std::nullopt;
    }

    const uint8_t obu_type = (obu_header & kAv1ObuTypeMask) >> 3;
    if ((obu_header & kAv1ObuExtensionFlag) != 0) {
      if (pos >= data.size()) {
        return std::nullopt;
      }
      ++pos;
    }

    size_t payload_size = data.size() - pos;
    if ((obu_header & kAv1ObuSizePresentBit) != 0) {
      uint64_t leb_payload_size = 0;
      if (!ReadLeb128(data, pos, leb_payload_size) ||
          leb_payload_size > data.size() - pos) {
        return std::nullopt;
      }
      payload_size = static_cast<size_t>(leb_payload_size);
    }

    if (obu_type == kAv1ObuTypeFrame &&
        payload_size >= 1 + sizeof(kEncryptedPayloadMagic)) {
      auto frame_payload = data.subview(pos, payload_size);
      if ((frame_payload[0] == kSyntheticKeyFrameHeaderByte ||
           frame_payload[0] == kSyntheticInterFrameHeaderByte) &&
          std::memcmp(frame_payload.data() + 1, kEncryptedPayloadMagic,
                      sizeof(kEncryptedPayloadMagic)) == 0) {
        auto payload_start =
            frame_payload.begin() + 1 + sizeof(kEncryptedPayloadMagic);
        return std::vector<uint8_t>(payload_start, frame_payload.end());
      }
    }

    pos += payload_size;
  }

  return std::nullopt;
}

}  // namespace av1
}  // namespace livekit_ffi
