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

#include "livekit/fec_controller.h"

#include <algorithm>
#include <cstdlib>

#include "modules/include/module_fec_types.h"
#include "rtc_base/logging.h"
#include "webrtc-sys/src/fec_controller.rs.h"

namespace livekit_ffi {

FecGlobalState& FecGlobalState::Instance() {
  static FecGlobalState* instance = new FecGlobalState();
  return *instance;
}

bool FecGlobalState::SetFieldTrials(const std::string& field_trials) {
  if (factory_created_.load()) {
    return false;
  }
  std::lock_guard<std::mutex> lock(mutex_);
  field_trials_ = field_trials;
  return true;
}

std::string FecGlobalState::BuildFieldTrialsString() {
  std::string trials;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    trials = field_trials_;
  }
  if (const char* env_trials = std::getenv("LK_WEBRTC_FIELD_TRIALS")) {
    trials += env_trials;
  }
  return trials;
}

void FecGlobalState::MarkFactoryCreated() {
  factory_created_.store(true);
}

void FecGlobalState::RegisterController(FixedRateFecController* controller) {
  std::lock_guard<std::mutex> lock(mutex_);
  controllers_.insert(controller);
}

void FecGlobalState::DeregisterController(FixedRateFecController* controller) {
  std::lock_guard<std::mutex> lock(mutex_);
  controllers_.erase(controller);
}

void FecGlobalState::AggregateMetrics(uint32_t& sent_video_rate_bps,
                                      uint32_t& sent_fec_rate_bps,
                                      uint32_t& sent_nack_rate_bps,
                                      uint32_t& active_streams) {
  sent_video_rate_bps = 0;
  sent_fec_rate_bps = 0;
  sent_nack_rate_bps = 0;
  active_streams = 0;

  std::lock_guard<std::mutex> lock(mutex_);
  for (const auto* controller : controllers_) {
    sent_video_rate_bps += controller->sent_video_rate_bps();
    sent_fec_rate_bps += controller->sent_fec_rate_bps();
    sent_nack_rate_bps += controller->sent_nack_rate_bps();
    active_streams++;
  }
}

// ---------------------------------------------------------------------------

FixedRateFecController::FixedRateFecController() {
  FecGlobalState::Instance().RegisterController(this);
}

FixedRateFecController::~FixedRateFecController() {
  FecGlobalState::Instance().DeregisterController(this);
}

void FixedRateFecController::SetProtectionCallback(
    webrtc::VCMProtectionCallback* protection_callback) {
  protection_callback_.store(protection_callback);
}

void FixedRateFecController::SetProtectionMethod(bool enable_fec,
                                                 bool enable_nack) {
  fec_negotiated_.store(enable_fec);
  RTC_LOG(LS_INFO) << "FixedRateFecController::SetProtectionMethod fec="
                   << enable_fec << " nack=" << enable_nack;
}

void FixedRateFecController::SetEncodingData(size_t width,
                                             size_t height,
                                             size_t num_temporal_layers,
                                             size_t max_payload_size) {}

uint32_t FixedRateFecController::UpdateFecRates(
    uint32_t estimated_bitrate_bps,
    int actual_framerate,
    uint8_t fraction_lost,
    std::vector<bool> loss_mask_vector,
    int64_t round_trip_time_ms) {
  auto& state = FecGlobalState::Instance();

  webrtc::FecProtectionParams delta_params;
  webrtc::FecProtectionParams key_params;
  if (state.enabled.load() && fec_negotiated_.load()) {
    int fec_rate = std::clamp(state.fec_rate.load(), 0, 255);
    int max_frames = std::clamp(state.max_fec_frames.load(), 1, 48);
    webrtc::FecMaskType mask_type = state.bursty_mask.load()
                                        ? webrtc::FecMaskType::kFecMaskBursty
                                        : webrtc::FecMaskType::kFecMaskRandom;
    delta_params.fec_rate = fec_rate;
    delta_params.max_fec_frames = max_frames;
    delta_params.fec_mask_type = mask_type;
    key_params = delta_params;
  }

  uint32_t sent_video_rate_bps = 0;
  uint32_t sent_nack_rate_bps = 0;
  uint32_t sent_fec_rate_bps = 0;
  if (auto* callback = protection_callback_.load()) {
    callback->ProtectionRequest(&delta_params, &key_params,
                                &sent_video_rate_bps, &sent_nack_rate_bps,
                                &sent_fec_rate_bps);
  }
  sent_video_rate_bps_.store(sent_video_rate_bps);
  sent_nack_rate_bps_.store(sent_nack_rate_bps);
  sent_fec_rate_bps_.store(sent_fec_rate_bps);

  if (!state.enabled.load() || !fec_negotiated_.load()) {
    return estimated_bitrate_bps;
  }

  // Reserve headroom for the protection overhead so the encoder target plus
  // FEC stays within the estimated link capacity, mirroring
  // FecControllerDefault. Until rates have been measured fall back to the
  // configured protection ratio.
  float overhead_rate;
  uint32_t sent_total =
      sent_video_rate_bps + sent_nack_rate_bps + sent_fec_rate_bps;
  if (sent_total > 0) {
    overhead_rate = static_cast<float>(sent_nack_rate_bps + sent_fec_rate_bps) /
                    static_cast<float>(sent_total);
  } else {
    int fec_rate = std::clamp(state.fec_rate.load(), 0, 255);
    overhead_rate = static_cast<float>(fec_rate) / (255.0f + fec_rate);
  }
  overhead_rate = std::min(overhead_rate, 0.5f);
  return static_cast<uint32_t>(estimated_bitrate_bps * (1.0f - overhead_rate));
}

void FixedRateFecController::UpdateWithEncodedData(
    size_t encoded_image_length,
    webrtc::VideoFrameType encoded_image_frametype) {}

bool FixedRateFecController::UseLossVectorMask() {
  return false;
}

// ---------------------------------------------------------------------------

std::unique_ptr<webrtc::FecController>
LkFecControllerFactory::CreateFecController(const webrtc::Environment& env) {
  return std::make_unique<FixedRateFecController>();
}

// ---------------------------------------------------------------------------

void set_fec_controller_config(FecControllerConfig config) {
  auto& state = FecGlobalState::Instance();
  state.enabled.store(config.enabled);
  state.fec_rate.store(config.fec_rate);
  state.max_fec_frames.store(config.max_fec_frames);
  state.bursty_mask.store(config.bursty_mask);
  RTC_LOG(LS_INFO) << "FlexFEC controller config enabled=" << config.enabled
                   << " fec_rate=" << config.fec_rate
                   << " max_fec_frames=" << config.max_fec_frames
                   << " bursty_mask=" << config.bursty_mask;
}

FecSenderMetrics fec_sender_metrics() {
  FecSenderMetrics metrics{};
  FecGlobalState::Instance().AggregateMetrics(
      metrics.sent_video_rate_bps, metrics.sent_fec_rate_bps,
      metrics.sent_nack_rate_bps, metrics.active_streams);
  return metrics;
}

bool set_field_trials(rust::String field_trials) {
  return FecGlobalState::Instance().SetFieldTrials(
      std::string(field_trials));
}

}  // namespace livekit_ffi
