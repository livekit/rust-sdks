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

#include <atomic>
#include <memory>
#include <mutex>
#include <set>
#include <string>

#include "api/environment/environment.h"
#include "api/fec_controller.h"
#include "rust/cxx.h"

namespace livekit_ffi {

struct FecControllerConfig;
struct FecSenderMetrics;

// Process wide FlexFEC state. The PeerConnectionFactory is a process
// singleton in the SDK, so FEC configuration is process wide as well. The
// protection parameters are runtime adjustable through atomics, field trials
// can only be applied before the factory is created.
class FecGlobalState {
 public:
  static FecGlobalState& Instance();

  // configuration (set via the cxx bridge)
  std::atomic<bool> enabled{false};
  std::atomic<int> fec_rate{38};         // 0..255, ~15%
  std::atomic<int> max_fec_frames{6};    // frames per protection block
  std::atomic<bool> bursty_mask{false};  // bursty vs random loss mask

  // Returns false when the factory already exists and the trials cannot
  // take effect anymore.
  bool SetFieldTrials(const std::string& field_trials);
  // Field trials string merged with the LK_WEBRTC_FIELD_TRIALS environment
  // variable, consumed at factory creation.
  std::string BuildFieldTrialsString();
  void MarkFactoryCreated();
  bool IsFactoryCreated() const { return factory_created_.load(); }

  void RegisterController(class FixedRateFecController* controller);
  void DeregisterController(class FixedRateFecController* controller);
  void AggregateMetrics(uint32_t& sent_video_rate_bps,
                        uint32_t& sent_fec_rate_bps,
                        uint32_t& sent_nack_rate_bps,
                        uint32_t& active_streams);

 private:
  FecGlobalState() = default;

  std::atomic<bool> factory_created_{false};
  std::mutex mutex_;
  std::string field_trials_;
  std::set<class FixedRateFecController*> controllers_;
};

// FecController that requests a constant protection rate whenever FEC has
// been negotiated for the stream, unlike webrtc::FecControllerDefault which
// only ramps protection after loss has been observed.
class FixedRateFecController : public webrtc::FecController {
 public:
  FixedRateFecController();
  ~FixedRateFecController() override;

  void SetProtectionCallback(
      webrtc::VCMProtectionCallback* protection_callback) override;
  void SetProtectionMethod(bool enable_fec, bool enable_nack) override;
  void SetEncodingData(size_t width,
                       size_t height,
                       size_t num_temporal_layers,
                       size_t max_payload_size) override;
  uint32_t UpdateFecRates(uint32_t estimated_bitrate_bps,
                          int actual_framerate,
                          uint8_t fraction_lost,
                          std::vector<bool> loss_mask_vector,
                          int64_t round_trip_time_ms) override;
  void UpdateWithEncodedData(
      size_t encoded_image_length,
      webrtc::VideoFrameType encoded_image_frametype) override;
  bool UseLossVectorMask() override;

  uint32_t sent_video_rate_bps() const { return sent_video_rate_bps_.load(); }
  uint32_t sent_fec_rate_bps() const { return sent_fec_rate_bps_.load(); }
  uint32_t sent_nack_rate_bps() const { return sent_nack_rate_bps_.load(); }

 private:
  std::atomic<webrtc::VCMProtectionCallback*> protection_callback_{nullptr};
  std::atomic<bool> fec_negotiated_{false};
  std::atomic<uint32_t> sent_video_rate_bps_{0};
  std::atomic<uint32_t> sent_fec_rate_bps_{0};
  std::atomic<uint32_t> sent_nack_rate_bps_{0};
};

class LkFecControllerFactory : public webrtc::FecControllerFactoryInterface {
 public:
  std::unique_ptr<webrtc::FecController> CreateFecController(
      const webrtc::Environment& env) override;
};

// cxx bridge entry points
void set_fec_controller_config(FecControllerConfig config);
FecSenderMetrics fec_sender_metrics();
bool set_field_trials(rust::String field_trials);

}  // namespace livekit_ffi
