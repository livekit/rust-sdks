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

}  // namespace av1
}  // namespace livekit_ffi
