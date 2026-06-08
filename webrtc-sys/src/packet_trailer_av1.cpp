/*
 * Copyright 2025 LiveKit, Inc.
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
#include <string>

namespace livekit_ffi {
namespace av1 {

namespace {

constexpr uint8_t kAv1ObuSizePresentBit = 0b0000'0010;
constexpr uint8_t kAv1ObuExtensionFlag = 0b0000'0100;
constexpr uint8_t kAv1ObuTypeMask = 0b0111'1000;
constexpr uint8_t kAv1ObuTypeSequenceHeader = 1;
constexpr uint8_t kAv1ObuTypeTemporalDelimiter = 2;
constexpr uint8_t kAv1ObuTypeMetadata = 5;
constexpr uint64_t kAv1MetadataTypeLiveKitPacketTrailer = 31;

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

}  // namespace av1
}  // namespace livekit_ffi
