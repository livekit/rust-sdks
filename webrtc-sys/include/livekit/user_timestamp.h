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

#include <stdint.h>

#include <atomic>
#include <deque>
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <vector>

#include "absl/types/optional.h"
#include "api/frame_transformer_interface.h"
#include "api/rtp_sender_interface.h"
#include "api/rtp_receiver_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

// Forward declarations to avoid circular includes
// (video_track.h -> user_timestamp.h -> peer_connection.h -> media_stream.h -> video_track.h)
namespace livekit_ffi {
class PeerConnectionFactory;
class RtpSender;
class RtpReceiver;
}  // namespace livekit_ffi

namespace livekit_ffi {

// Magic bytes to identify user timestamp trailers: "LKTS" (LiveKit TimeStamp)
constexpr uint8_t kUserTimestampMagic[4] = {'L', 'K', 'T', 'S'};
constexpr size_t kUserTimestampTrailerSize =
    16;  // 8 bytes timestamp + 4 bytes frame_id + 4 bytes magic

struct FrameMetadata {
  int64_t user_timestamp_us;
  uint32_t frame_id;
};

/// Frame transformer that appends/extracts user timestamp trailers.
/// This transformer can be used standalone or in conjunction with e2ee.
///
/// On the send side, user timestamps are stored in an internal map keyed
/// by capture timestamp (microseconds).  When TransformSend fires it
/// looks up the user timestamp via the frame's CaptureTime().
///
/// On the receive side, extracted frame metadata is stored in an
/// internal map keyed by RTP timestamp (uint32_t).  Decoded frames can
/// look up their metadata via lookup_frame_metadata(rtp_ts).
class UserTimestampTransformer : public webrtc::FrameTransformerInterface {
 public:
  enum class Direction { kSend, kReceive };

  explicit UserTimestampTransformer(Direction direction);
  ~UserTimestampTransformer() override = default;

  // FrameTransformerInterface implementation
  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;
  void RegisterTransformedFrameCallback(
      rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override;
  void RegisterTransformedFrameSinkCallback(
      rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override;
  void UnregisterTransformedFrameCallback() override;
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override;

  /// Enable/disable timestamp embedding
  void set_enabled(bool enabled);
  bool enabled() const;

  /// Lookup the frame metadata associated with a given RTP timestamp.
  /// Returns the metadata if found, nullopt otherwise.
  /// The entry is removed from the map after lookup.
  std::optional<FrameMetadata> lookup_frame_metadata(uint32_t rtp_timestamp);

  /// Store frame metadata for a given capture timestamp (sender side).
  /// Called from VideoTrackSource::on_captured_frame with the
  /// TimestampAligner-adjusted timestamp, which matches CaptureTime()
  /// in the encoder pipeline.
  void store_frame_metadata(int64_t capture_timestamp_us,
                            int64_t user_timestamp_us,
                            uint32_t frame_id);

 private:
  void TransformSend(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);
  void TransformReceive(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);

  /// Append frame metadata trailer to frame data
  std::vector<uint8_t> AppendTrailer(
      rtc::ArrayView<const uint8_t> data,
      int64_t user_timestamp_us,
      uint32_t frame_id);

  /// Extract and remove frame metadata trailer from frame data
  std::optional<FrameMetadata> ExtractTrailer(
      rtc::ArrayView<const uint8_t> data,
      std::vector<uint8_t>& out_data);

  const Direction direction_;
  std::atomic<bool> enabled_{true};
  mutable webrtc::Mutex mutex_;
  rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::unordered_map<uint32_t,
                     rtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sink_callbacks_;
  // Send-side map: capture timestamp (us) -> frame metadata.
  // Populated by store_frame_metadata(), consumed by TransformSend()
  // via CaptureTime() lookup.
  mutable webrtc::Mutex send_map_mutex_;
  mutable std::unordered_map<int64_t, FrameMetadata> send_map_;
  mutable std::deque<int64_t> send_map_order_;
  static constexpr size_t kMaxSendMapEntries = 300;

  // Receive-side map: RTP timestamp -> frame metadata.
  // Keyed by RTP timestamp so decoded frames can look up their
  // metadata regardless of frame drops or reordering.
  mutable webrtc::Mutex recv_map_mutex_;
  mutable std::unordered_map<uint32_t, FrameMetadata> recv_map_;
  mutable std::deque<uint32_t> recv_map_order_;
  static constexpr size_t kMaxRecvMapEntries = 300;
};

/// Wrapper class for Rust FFI that manages user timestamp transformers.
class UserTimestampHandler {
 public:
  UserTimestampHandler(
      std::shared_ptr<RtcRuntime> rtc_runtime,
      rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  UserTimestampHandler(
      std::shared_ptr<RtcRuntime> rtc_runtime,
      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

  ~UserTimestampHandler() = default;

  /// Enable/disable timestamp embedding
  void set_enabled(bool enabled) const;
  bool enabled() const;

  /// Lookup the user timestamp for a given RTP timestamp (receiver side).
  /// Returns -1 if not found. The entry is removed after lookup.
  /// Also caches the frame_id for retrieval via last_lookup_frame_id().
  int64_t lookup_user_timestamp(uint32_t rtp_timestamp) const;

  /// Returns the frame_id from the most recent successful
  /// lookup_user_timestamp() call. Returns 0 if no lookup succeeded.
  uint32_t last_lookup_frame_id() const;

  /// Store frame metadata for a given capture timestamp (sender side).
  void store_frame_metadata(int64_t capture_timestamp_us,
                            int64_t user_timestamp_us,
                            uint32_t frame_id) const;

  /// Access the underlying transformer for chaining.
  rtc::scoped_refptr<UserTimestampTransformer> transformer() const;

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  rtc::scoped_refptr<UserTimestampTransformer> transformer_;
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  mutable uint32_t last_frame_id_{0};
};

// Factory functions for Rust FFI

std::shared_ptr<UserTimestampHandler> new_user_timestamp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpSender> sender);

std::shared_ptr<UserTimestampHandler> new_user_timestamp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<RtpReceiver> receiver);

}  // namespace livekit_ffi
