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
 
#include "av1_bitstream.h"

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
    if (shift > 56) {
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
  // A standalone IVF frame header must describe exactly the remaining buffer.
  // Otherwise valid low-overhead OBUs can be mistaken for a length prefix.
  if (declared_size == 0 || declared_size != packet->size() - 12) {
    return false;
  }

  const uint8_t* payload = packet->data() + 12;
  if (ParseObus(payload, declared_size).empty()) {
    return false;
  }

  packet->erase(packet->begin(), packet->begin() + 12);
  packet->resize(declared_size);
  return true;
}

bool ReadBoundedLeb128(const uint8_t* data,
                       size_t end,
                       size_t* offset,
                       uint64_t* value) {
  const size_t before = offset ? *offset : 0;
  if (!ReadLeb128(data, end, offset, value)) {
    return false;
  }
  return offset && *offset > before && *offset <= end;
}

bool IsCompleteObu(const uint8_t* data, size_t len);

// Annex-B wraps one temporal unit in a LEB128 size, then frame units, then
// length-delimited OBUs. WebRTC's RTP packetizer wants the OBU bytes only.
bool AppendAnnexBObus(const uint8_t* data,
                      size_t len,
                      std::vector<uint8_t>* low_overhead) {
  if (!data || len == 0 || !low_overhead) {
    return false;
  }

  size_t offset = 0;
  uint64_t temporal_unit_size = 0;
  if (!ReadBoundedLeb128(data, len, &offset, &temporal_unit_size) ||
      temporal_unit_size == 0 || temporal_unit_size != len - offset) {
    return false;
  }
  const size_t temporal_unit_end =
      offset + static_cast<size_t>(temporal_unit_size);

  while (offset < temporal_unit_end) {
    uint64_t frame_unit_size = 0;
    if (!ReadBoundedLeb128(data, temporal_unit_end, &offset, &frame_unit_size) ||
        frame_unit_size == 0 || frame_unit_size > temporal_unit_end - offset) {
      return false;
    }
    const size_t frame_unit_end = offset + static_cast<size_t>(frame_unit_size);

    while (offset < frame_unit_end) {
      uint64_t obu_size = 0;
      if (!ReadBoundedLeb128(data, frame_unit_end, &offset, &obu_size) ||
          obu_size == 0 || obu_size > frame_unit_end - offset) {
        return false;
      }
      const size_t obu_start = offset;
      const size_t obu_end = offset + static_cast<size_t>(obu_size);
      if (!IsCompleteObu(data + obu_start, static_cast<size_t>(obu_size))) {
        return false;
      }
      low_overhead->insert(low_overhead->end(), data + obu_start,
                           data + obu_end);
      offset = obu_end;
    }

    if (offset != frame_unit_end) {
      return false;
    }
  }

  return offset == len && !low_overhead->empty() &&
         !ParseObus(low_overhead->data(), low_overhead->size()).empty();
}

bool IsCompleteObu(const uint8_t* data, size_t len) {
  if (!data || len == 0) {
    return false;
  }
  size_t offset = 0;
  const uint8_t header = data[offset++];
  if (ObuHasExtension(header)) {
    if (offset >= len) {
      return false;
    }
    ++offset;
  }
  if (ObuHasSize(header)) {
    uint64_t payload_size = 0;
    if (!ReadLeb128(data, len, &offset, &payload_size) ||
        payload_size > len - offset) {
      return false;
    }
    offset += static_cast<size_t>(payload_size);
  } else {
    offset = len;
  }
  return offset == len;
}

bool ConvertAnnexBToLowOverhead(std::vector<uint8_t>* packet) {
  if (!packet || packet->empty()) {
    return false;
  }

  std::vector<uint8_t> low_overhead;
  low_overhead.reserve(packet->size());
  if (!AppendAnnexBObus(packet->data(), packet->size(), &low_overhead)) {
    return false;
  }

  packet->swap(low_overhead);
  return true;
}

bool StripNonTransferObus(std::vector<uint8_t>* packet) {
  if (!packet || packet->empty()) {
    return false;
  }

  const std::vector<ObuSpan> obus = ParseObus(packet->data(), packet->size());
  if (obus.empty()) {
    return false;
  }

  size_t transfer_size = 0;
  bool already_contiguous = true;
  size_t next_offset = 0;
  for (const ObuSpan& obu : obus) {
    transfer_size += obu.total_size;
    already_contiguous = already_contiguous && obu.offset == next_offset;
    next_offset = obu.offset + obu.total_size;
  }

  if (transfer_size == packet->size() && already_contiguous) {
    return false;
  }

  std::vector<uint8_t> filtered;
  filtered.reserve(transfer_size);
  for (const ObuSpan& obu : obus) {
    filtered.insert(filtered.end(), packet->begin() + obu.offset,
                    packet->begin() + obu.offset + obu.total_size);
  }
  packet->swap(filtered);
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

namespace {

// MSB-first bit reader over an OBU payload.
class BitReader {
 public:
  BitReader(const uint8_t* data, size_t len) : data_(data), len_(len) {}

  bool Read(int bits, uint64_t* value) {
    uint64_t v = 0;
    for (int i = 0; i < bits; ++i) {
      if (pos_ >= len_ * 8) {
        return false;
      }
      v = (v << 1) | ((data_[pos_ / 8] >> (7 - pos_ % 8)) & 1);
      ++pos_;
    }
    *value = v;
    return true;
  }

  bool Skip(int bits) {
    uint64_t ignored;
    return Read(bits, &ignored);
  }

  // uvlc() from the AV1 spec 4.10.3.
  bool SkipUvlc() {
    int leading_zeros = 0;
    uint64_t bit = 0;
    while (true) {
      if (!Read(1, &bit)) {
        return false;
      }
      if (bit) {
        break;
      }
      if (++leading_zeros >= 32) {
        return false;
      }
    }
    return Skip(leading_zeros);
  }

 private:
  const uint8_t* data_;
  size_t len_;
  size_t pos_ = 0;
};

// sequence_header_obu() up to max_frame_height_minus_1 (AV1 spec 5.5.1).
bool ReadSequenceHeaderFrameSize(BitReader& r, int* width, int* height) {
  uint64_t reduced_still_picture_header = 0;
  if (!r.Skip(3) || !r.Skip(1) || !r.Read(1, &reduced_still_picture_header)) {
    return false;  // seq_profile, still_picture
  }
  if (reduced_still_picture_header) {
    if (!r.Skip(5)) {
      return false;  // seq_level_idx[0]
    }
  } else {
    uint64_t timing_info_present = 0;
    uint64_t decoder_model_info_present = 0;
    uint64_t buffer_delay_length_minus_1 = 0;
    if (!r.Read(1, &timing_info_present)) {
      return false;
    }
    if (timing_info_present) {
      uint64_t equal_picture_interval = 0;
      // num_units_in_display_tick, time_scale
      if (!r.Skip(32) || !r.Skip(32) || !r.Read(1, &equal_picture_interval)) {
        return false;
      }
      if (equal_picture_interval && !r.SkipUvlc()) {
        return false;  // num_ticks_per_picture_minus_1
      }
      if (!r.Read(1, &decoder_model_info_present)) {
        return false;
      }
      if (decoder_model_info_present) {
        // buffer_delay_length_minus_1, num_units_in_decoding_tick,
        // buffer_removal_time_length_minus_1,
        // frame_presentation_time_length_minus_1
        if (!r.Read(5, &buffer_delay_length_minus_1) || !r.Skip(32) ||
            !r.Skip(5) || !r.Skip(5)) {
          return false;
        }
      }
    }
    uint64_t initial_display_delay_present = 0;
    uint64_t operating_points_cnt_minus_1 = 0;
    if (!r.Read(1, &initial_display_delay_present) ||
        !r.Read(5, &operating_points_cnt_minus_1)) {
      return false;
    }
    for (uint64_t i = 0; i <= operating_points_cnt_minus_1; ++i) {
      uint64_t seq_level_idx = 0;
      if (!r.Skip(12) || !r.Read(5, &seq_level_idx)) {
        return false;  // operating_point_idc, seq_level_idx
      }
      if (seq_level_idx > 7 && !r.Skip(1)) {
        return false;  // seq_tier
      }
      if (decoder_model_info_present) {
        uint64_t decoder_model_present = 0;
        if (!r.Read(1, &decoder_model_present)) {
          return false;
        }
        if (decoder_model_present) {
          const int n = static_cast<int>(buffer_delay_length_minus_1) + 1;
          // decoder_buffer_delay, encoder_buffer_delay, low_delay_mode_flag
          if (!r.Skip(n) || !r.Skip(n) || !r.Skip(1)) {
            return false;
          }
        }
      }
      if (initial_display_delay_present) {
        uint64_t present_for_op = 0;
        if (!r.Read(1, &present_for_op)) {
          return false;
        }
        if (present_for_op && !r.Skip(4)) {
          return false;  // initial_display_delay_minus_1
        }
      }
    }
  }
  uint64_t width_bits_minus_1 = 0;
  uint64_t height_bits_minus_1 = 0;
  uint64_t max_width_minus_1 = 0;
  uint64_t max_height_minus_1 = 0;
  if (!r.Read(4, &width_bits_minus_1) || !r.Read(4, &height_bits_minus_1) ||
      !r.Read(static_cast<int>(width_bits_minus_1) + 1, &max_width_minus_1) ||
      !r.Read(static_cast<int>(height_bits_minus_1) + 1, &max_height_minus_1)) {
    return false;
  }
  *width = static_cast<int>(max_width_minus_1) + 1;
  *height = static_cast<int>(max_height_minus_1) + 1;
  return true;
}

}  // namespace

bool ParseSequenceHeaderFrameSize(const uint8_t* data,
                                  size_t len,
                                  int* width,
                                  int* height) {
  for (const ObuSpan& obu : ParseObus(data, len)) {
    if (obu.type != kObuTypeSequenceHeader) {
      continue;
    }
    size_t offset = obu.offset;
    const size_t end = obu.offset + obu.total_size;
    const uint8_t header = data[offset++];
    if (ObuHasExtension(header)) {
      ++offset;
    }
    if (obu.has_size_field) {
      uint64_t ignored = 0;
      if (!ReadLeb128(data, end, &offset, &ignored)) {
        return false;
      }
    }
    if (offset > end) {
      return false;
    }
    BitReader reader(data + offset, end - offset);
    return ReadSequenceHeaderFrameSize(reader, width, height);
  }
  return false;
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

void ConvertAnnexBToLowOverheadIfPresent(std::vector<uint8_t>* packet) {
  ConvertAnnexBToLowOverhead(packet);
}

void StripNonTransferObusIfPresent(std::vector<uint8_t>* packet) {
  StripNonTransferObus(packet);
}

void NormalizeForRtp(std::vector<uint8_t>* packet) {
  StripIvfFrameHeaderIfPresent(packet);
  ConvertAnnexBToLowOverheadIfPresent(packet);
  StripNonTransferObusIfPresent(packet);
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
