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

#pragma once

#include <cstdint>
#include <optional>
#include <vector>

#include "api/array_view.h"
#include "api/frame_transformer_interface.h"
#include "livekit/packet_trailer.h"

namespace livekit_ffi {
namespace av1 {

/// Returns true if the frame's MIME type identifies it as AV1.
bool IsAv1Frame(const webrtc::TransformableFrameInterface& frame);

/// Inserts a LiveKit packet-trailer metadata OBU into an AV1 temporal unit.
///
/// `trailer` is the already-built TLV trailer payload (see
/// [`PacketTrailerTransformer`]). The OBU is placed after any temporal
/// delimiter and sequence header OBUs so it is not mistaken for frame data.
std::vector<uint8_t> InsertTrailerObu(
    webrtc::ArrayView<const uint8_t> data,
    webrtc::ArrayView<const uint8_t> trailer);

/// Extracts and removes a LiveKit packet-trailer metadata OBU from an AV1
/// temporal unit.
///
/// On success the parsed metadata is returned and `out_data` receives the
/// frame data with the metadata OBU removed. Otherwise `out_data` receives
/// an unchanged copy of `data` and `std::nullopt` is returned.
std::optional<PacketTrailerMetadata> ExtractTrailer(
    webrtc::ArrayView<const uint8_t> data,
    std::vector<uint8_t>& out_data);

/// Returns the sequence header OBU contained in an AV1 temporal unit,
/// normalized to carry an explicit size field, or `std::nullopt` if the
/// data does not contain one (e.g. delta frames or non-AV1 payloads).
std::optional<std::vector<uint8_t>> ExtractSequenceHeaderObu(
    webrtc::ArrayView<const uint8_t> data);

/// Wraps an encrypted AV1 payload into a synthetic-but-valid AV1 temporal
/// unit so it survives RTP transport.
///
/// E2EE encrypts the entire AV1 payload, but `RtpPacketizerAv1` parses its
/// input as a sequence of OBUs: ciphertext fails to parse (dropping frames)
/// or is silently corrupted in transit (the depacketizer re-writes OBU size
/// fields), and SFUs lose keyframe detection, which requires the payload to
/// start with a sequence header OBU followed by a keyframe-flagged frame
/// OBU. This function hides the ciphertext inside a frame OBU behind a
/// synthetic frame header byte and magic bytes, prepending a sequence
/// header OBU on keyframes. Every emitted OBU carries an explicit size
/// field, which makes packetization round-trip byte-exact.
///
/// `sequence_header_obu` is the plaintext frame's own sequence header
/// (captured before encryption, see [`ExtractSequenceHeaderObu`]) so that
/// SFUs parsing it observe the stream's real parameters; when empty, a
/// minimal synthetic header is emitted instead. It is removed again by
/// [`UnwrapEncryptedPayload`], so decoders only ever see the encrypted
/// original.
std::vector<uint8_t> WrapEncryptedPayload(
    webrtc::ArrayView<const uint8_t> data,
    bool is_keyframe,
    webrtc::ArrayView<const uint8_t> sequence_header_obu);

/// Reverses [`WrapEncryptedPayload`], returning the encrypted payload carried
/// by the wrapper frame OBU, or `std::nullopt` if `data` is not a wrapped
/// payload (e.g. unencrypted passthrough or server-injected frames).
std::optional<std::vector<uint8_t>> UnwrapEncryptedPayload(
    webrtc::ArrayView<const uint8_t> data);

}  // namespace av1
}  // namespace livekit_ffi
