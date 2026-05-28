#include "jetson_av1_bitstream.h"

#include <algorithm>

namespace livekit {
namespace av1 {
namespace {

constexpr uint8_t kObuSizePresentBit = 0b0'0000'010;
constexpr int kObuTypeSequenceHeader = 1;
constexpr int kObuTypeTemporalDelimiter = 2;
constexpr int kObuTypeTileList = 8;
constexpr int kObuTypePadding = 15;

bool ObuHasExtension(uint8_t obu_header) {
  return (obu_header & 0b0'0000'100) != 0;
}

bool ObuHasSize(uint8_t obu_header) {
  return (obu_header & kObuSizePresentBit) != 0;
}

int ObuType(uint8_t obu_header) {
  return (obu_header & 0b0'1111'000) >> 3;
}

bool ReadLeb128(const uint8_t* data, size_t len, size_t* offset, uint64_t* value) {
  if (!data || !offset || !value || *offset >= len) {
    return false;
  }
  uint64_t result = 0;
  int shift = 0;
  while (*offset < len) {
    const uint8_t byte = data[(*offset)++];
    result |= static_cast<uint64_t>(byte & 0x7F) << shift;
    if ((byte & 0x80) == 0) {
      *value = result;
      return true;
    }
    shift += 7;
    if (shift > 28) {
      return false;
    }
  }
  return false;
}

bool ShouldTransferObu(int obu_type) {
  return obu_type != kObuTypeTemporalDelimiter && obu_type != kObuTypeTileList &&
         obu_type != kObuTypePadding;
}

uint32_t ReadLittleEndianUint32(const std::vector<uint8_t>& data) {
  if (data.size() < 4) {
    return 0;
  }
  return static_cast<uint32_t>(data[0]) |
         (static_cast<uint32_t>(data[1]) << 8) |
         (static_cast<uint32_t>(data[2]) << 16) |
         (static_cast<uint32_t>(data[3]) << 24);
}

bool StripIvfFrameHeader(std::vector<uint8_t>* packet) {
  if (!packet || packet->size() < 12) {
    return false;
  }

  const uint32_t declared_size = ReadLittleEndianUint32(*packet);
  if (declared_size == 0 || declared_size != packet->size() - 12) {
    return false;
  }

  const uint8_t* payload = packet->data() + 12;
  if (ParseObus(payload, declared_size).empty()) {
    return false;
  }

  packet->erase(packet->begin(), packet->begin() + 12);
  return true;
}

}  // namespace

std::vector<ObuSpan> ParseObus(const uint8_t* data, size_t len) {
  std::vector<ObuSpan> result;
  if (!data || len == 0) {
    return result;
  }

  size_t offset = 0;
  while (offset < len) {
    ObuSpan obu;
    obu.offset = offset;
    const uint8_t header = data[offset++];
    obu.type = ObuType(header);
    obu.has_size_field = ObuHasSize(header);

    if (ObuHasExtension(header)) {
      if (offset >= len) {
        return {};
      }
      ++offset;
    }

    if (obu.has_size_field) {
      uint64_t payload_size = 0;
      if (!ReadLeb128(data, len, &offset, &payload_size) ||
          payload_size > len - offset) {
        return {};
      }
      offset += static_cast<size_t>(payload_size);
    } else {
      offset = len;
    }

    obu.total_size = offset - obu.offset;
    if (ShouldTransferObu(obu.type)) {
      result.push_back(obu);
    }
  }

  return result;
}

bool HasSequenceHeaderObu(const uint8_t* data, size_t len) {
  for (const ObuSpan& obu : ParseObus(data, len)) {
    if (obu.type == kObuTypeSequenceHeader) {
      return true;
    }
  }
  return false;
}

bool ExtractSequenceHeaderObu(const uint8_t* data,
                              size_t len,
                              std::vector<uint8_t>* out) {
  if (!out) {
    return false;
  }
  for (const ObuSpan& obu : ParseObus(data, len)) {
    if (obu.type != kObuTypeSequenceHeader) {
      continue;
    }
    if (obu.offset + obu.total_size > len) {
      return false;
    }
    out->assign(data + obu.offset, data + obu.offset + obu.total_size);
    return true;
  }
  return false;
}

void EnsureSequenceHeaderOnKeyframe(
    std::vector<uint8_t>* packet,
    const std::vector<uint8_t>& cached_seq_header) {
  if (!packet || packet->empty() || cached_seq_header.empty()) {
    return;
  }
  if (HasSequenceHeaderObu(packet->data(), packet->size())) {
    return;
  }
  std::vector<uint8_t> merged;
  merged.reserve(cached_seq_header.size() + packet->size());
  merged.insert(merged.end(), cached_seq_header.begin(), cached_seq_header.end());
  merged.insert(merged.end(), packet->begin(), packet->end());
  packet->swap(merged);
}

void StripIvfFrameHeaderIfPresent(std::vector<uint8_t>* packet) {
  if (!packet || packet->size() < 12) {
    return;
  }
  // IVF file header starts with the "DKIF" signature. Per-frame IVF headers do
  // not include that signature and are not valid OBU streams for WebRTC.
  if (packet->size() >= 32 && packet->at(0) == 'D' && packet->at(1) == 'K' &&
      packet->at(2) == 'I' && packet->at(3) == 'F') {
    if (packet->size() <= 32) {
      packet->clear();
      return;
    }
    packet->erase(packet->begin(), packet->begin() + 32);
  }
  StripIvfFrameHeader(packet);
}

bool IsWebRtcParseable(const uint8_t* data, size_t len) {
  if (!data || len == 0) {
    return false;
  }
  const std::vector<ObuSpan> obus = ParseObus(data, len);
  return !obus.empty();
}

}  // namespace av1
}  // namespace livekit
