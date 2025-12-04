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
#include "api/scoped_refptr.h"
#include "livekit/peer_connection.h"
#include "livekit/peer_connection_factory.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit {

// Magic bytes to identify sensor timestamp trailers: "LKTS" (LiveKit TimeStamp)
constexpr uint8_t kSensorTimestampMagic[4] = {'L', 'K', 'T', 'S'};
constexpr size_t kSensorTimestampTrailerSize = 12;  // 8 bytes timestamp + 4 bytes magic

/// Thread-safe FIFO queue for sensor timestamps.
/// Used on the sender side to pass sensor timestamps to the transformer.
/// Works on the assumption that frames are captured and encoded in order.
class SensorTimestampStore {
 public:
  SensorTimestampStore() = default;
  ~SensorTimestampStore() = default;

  /// Push a sensor timestamp to the queue.
  /// Call this when capturing a video frame with a sensor timestamp.
  void store(int64_t capture_timestamp_us, int64_t sensor_timestamp_us) const;

  /// Pop and return the next sensor timestamp from the queue.
  /// Returns -1 if the queue is empty.
  int64_t lookup(int64_t capture_timestamp_us) const;

  /// Pop the oldest entry if the queue has entries.
  /// Returns the sensor timestamp, or -1 if empty.
  int64_t pop() const;

  /// Peek at the oldest entry without removing it.
  /// Returns the sensor timestamp, or -1 if empty.
  int64_t peek() const;

  /// Clear old entries (older than the given threshold in microseconds).
  void prune(int64_t max_age_us) const;

 private:
  mutable webrtc::Mutex mutex_;
  struct Entry {
    int64_t capture_timestamp_us;
    int64_t sensor_timestamp_us;
  };
  mutable std::deque<Entry> entries_;
  static constexpr size_t kMaxEntries = 300;  // ~10 seconds at 30fps
};

/// Frame transformer that appends/extracts sensor timestamp trailers.
/// This transformer can be used standalone or in conjunction with e2ee.
class SensorTimestampTransformer
    : public webrtc::FrameTransformerInterface {
 public:
  enum class Direction { kSend, kReceive };

  SensorTimestampTransformer(Direction direction,
                             std::shared_ptr<SensorTimestampStore> store);
  ~SensorTimestampTransformer() override = default;

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

  /// Get the last received sensor timestamp (receiver side only)
  std::optional<int64_t> last_sensor_timestamp() const;

 private:
  void TransformSend(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);
  void TransformReceive(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame);

  /// Append sensor timestamp trailer to frame data
  std::vector<uint8_t> AppendTimestampTrailer(
      rtc::ArrayView<const uint8_t> data,
      int64_t sensor_timestamp_us);

  /// Extract and remove sensor timestamp trailer from frame data
  /// Returns the sensor timestamp if found, nullopt otherwise
  std::optional<int64_t> ExtractTimestampTrailer(
      rtc::ArrayView<const uint8_t> data,
      std::vector<uint8_t>& out_data);

  const Direction direction_;
  std::shared_ptr<SensorTimestampStore> store_;
  std::atomic<bool> enabled_{true};
  mutable webrtc::Mutex mutex_;
  rtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_;
  std::unordered_map<uint32_t,
                     rtc::scoped_refptr<webrtc::TransformedFrameCallback>>
      sink_callbacks_;
  mutable std::atomic<int64_t> last_sensor_timestamp_{0};
  mutable std::atomic<bool> has_last_sensor_timestamp_{false};
};

/// Wrapper class for Rust FFI that manages sensor timestamp transformers.
class SensorTimestampHandler {
 public:
  SensorTimestampHandler(std::shared_ptr<RtcRuntime> rtc_runtime,
                         std::shared_ptr<SensorTimestampStore> store,
                         rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  SensorTimestampHandler(std::shared_ptr<RtcRuntime> rtc_runtime,
                         std::shared_ptr<SensorTimestampStore> store,
                         rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

  ~SensorTimestampHandler() = default;

  /// Enable/disable timestamp embedding
  void set_enabled(bool enabled) const;
  bool enabled() const;

  /// Get the last received sensor timestamp (receiver side only)
  /// Returns -1 if no timestamp has been received yet
  int64_t last_sensor_timestamp() const;

  /// Check if a sensor timestamp has been received
  bool has_sensor_timestamp() const;

 private:
  std::shared_ptr<RtcRuntime> rtc_runtime_;
  rtc::scoped_refptr<SensorTimestampTransformer> transformer_;
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
};

// Factory functions for Rust FFI
std::shared_ptr<SensorTimestampStore> new_sensor_timestamp_store();

std::shared_ptr<SensorTimestampHandler> new_sensor_timestamp_sender(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<SensorTimestampStore> store,
    std::shared_ptr<RtpSender> sender);

std::shared_ptr<SensorTimestampHandler> new_sensor_timestamp_receiver(
    std::shared_ptr<PeerConnectionFactory> peer_factory,
    std::shared_ptr<SensorTimestampStore> store,
    std::shared_ptr<RtpReceiver> receiver);

}  // namespace livekit

