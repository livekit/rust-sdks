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

#include "api/environment/environment.h"
#include "api/fec_controller.h"
#include "api/fec_controller_override.h"

namespace livekit_ffi {

// Process-wide FlexFEC registry. Protection rates are bound to individual video
// send-stream controllers. FlexFEC negotiation is enabled on the shared
// PeerConnectionFactory.
class FecGlobalState {
 public:
  static FecGlobalState& Instance();

  void DeregisterController(class FixedRateFecController* controller);
  void BindController(class FixedRateFecController* controller,
                      webrtc::VCMProtectionCallback* protection_callback);
  void ConfigureController(
      webrtc::FecControllerOverride* fec_controller_override,
      int fec_rate);

 private:
  FecGlobalState() = default;

  std::mutex mutex_;
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

  void SetFecRate(int fec_rate) { fec_rate_.store(fec_rate); }

 private:
  std::atomic<webrtc::VCMProtectionCallback*> protection_callback_{nullptr};
  std::atomic<bool> fec_negotiated_{false};
  std::atomic<int> fec_rate_{0};
};

class LkFecControllerFactory : public webrtc::FecControllerFactoryInterface {
 public:
  std::unique_ptr<webrtc::FecController> CreateFecController(
      const webrtc::Environment& env) override;
};

}  // namespace livekit_ffi
