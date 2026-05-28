#ifndef WEBRTC_JETSON_AV1_BITSTREAM_H_
#define WEBRTC_JETSON_AV1_BITSTREAM_H_

#include <cstddef>
#include <cstdint>
#include <vector>

namespace livekit {
namespace av1 {

/// Parsed span of a single AV1 OBU inside a low-overhead bitstream.
struct ObuSpan {
  size_t offset = 0;
  size_t total_size = 0;
  int type = -1;
  bool has_size_field = false;
};

/// Parse AV1 OBUs using the same rules as WebRTC's `RtpPacketizerAv1`.
/// Returns an empty vector when the bitstream is malformed.
std::vector<ObuSpan> ParseObus(const uint8_t* data, size_t len);

/// Returns true when the bitstream contains an `OBU_SEQUENCE_HEADER`.
bool HasSequenceHeaderObu(const uint8_t* data, size_t len);

/// Extract the first sequence-header OBU bytes, if present.
bool ExtractSequenceHeaderObu(const uint8_t* data,
                              size_t len,
                              std::vector<uint8_t>* out);

/// Prepend a cached sequence-header OBU to a keyframe when the encoder omitted it.
void EnsureSequenceHeaderOnKeyframe(std::vector<uint8_t>* packet,
                                    const std::vector<uint8_t>& cached_seq_header);

/// Strip a per-frame IVF container header when present.
void StripIvfFrameHeaderIfPresent(std::vector<uint8_t>* packet);

/// Basic validation that WebRTC's AV1 packetizer can parse the bitstream.
bool IsWebRtcParseable(const uint8_t* data, size_t len);

}  // namespace av1
}  // namespace livekit

#endif  // WEBRTC_JETSON_AV1_BITSTREAM_H_
