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
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>

#include "api/environment/environment.h"
#include "api/fec_controller.h"
#include "api/fec_controller_override.h"
#include "rust/cxx.h"

namespace livekit_ffi {

struct FecSenderMetrics;

// Process-wide FlexFEC registry. Negotiation field trials belong to the shared
// PeerConnectionFactory, while protection rates are bound to individual video
// send-stream controllers.
class FecGlobalState {
 public:
  static FecGlobalState& Instance();

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
  void BindController(class FixedRateFecController* controller,
                      webrtc::VCMProtectionCallback* protection_callback);
  void ConfigureController(
      webrtc::FecControllerOverride* fec_controller_override,
      int fec_rate);
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
  std::map<const void*, class FixedRateFecController*> controllers_by_owner_;
  std::map<const void*, int> fec_rates_by_owner_;
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
  void SetFecRate(int fec_rate) { fec_rate_.store(fec_rate); }

 private:
  std::atomic<webrtc::VCMProtectionCallback*> protection_callback_{nullptr};
  std::atomic<bool> fec_negotiated_{false};
  std::atomic<int> fec_rate_{0};
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
FecSenderMetrics fec_sender_metrics();
bool set_field_trials(rust::String field_trials);

}  // namespace livekit_ffi
