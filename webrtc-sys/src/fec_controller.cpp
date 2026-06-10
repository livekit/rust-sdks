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

#include <atomic>
#include <memory>
#include <utility>
#include <vector>

#include "api/environment/environment.h"
#include "modules/video_coding/fec_controller_default.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit_ffi {
namespace {

webrtc::Mutex& GlobalFecOverrideMutex() {
  static webrtc::Mutex* mutex = new webrtc::Mutex();
  return *mutex;
}

FecOverrideOptions& GlobalFecOverrideStorage() {
  static FecOverrideOptions* options = new FecOverrideOptions();
  return *options;
}

FecOverrideOptions GlobalFecOverride() {
  webrtc::MutexLock lock(&GlobalFecOverrideMutex());
  return GlobalFecOverrideStorage();
}

// Intercepts ProtectionRequest from FecControllerDefault, rewrites the
// FecProtectionParams per the configured overrides, then forwards to the
// real callback (the video send stream).
class FecOverrideProtectionCallback : public webrtc::VCMProtectionCallback {
 public:
  explicit FecOverrideProtectionCallback(FecOverrideOptions options)
      : options_(options) {}

  // SetProtectionCallback (stream setup) and ProtectionRequest (rate updates)
  // can run on different threads, hence the atomic pointer.
  void SetRealCallback(webrtc::VCMProtectionCallback* callback) {
    real_callback_.store(callback, std::memory_order_release);
  }

  int ProtectionRequest(const webrtc::FecProtectionParams* delta_params,
                        const webrtc::FecProtectionParams* key_params,
                        uint32_t* sent_video_rate_bps,
                        uint32_t* sent_nack_rate_bps,
                        uint32_t* sent_fec_rate_bps) override {
    webrtc::VCMProtectionCallback* real =
        real_callback_.load(std::memory_order_acquire);
    if (!real)
      return -1;
    webrtc::FecProtectionParams delta = *delta_params;
    webrtc::FecProtectionParams key = *key_params;
    Apply(delta);
    Apply(key);
    return real->ProtectionRequest(&delta, &key, sent_video_rate_bps,
                                   sent_nack_rate_bps, sent_fec_rate_bps);
  }

  void SetRetransmissionMode(int retransmission_mode) override {
    if (auto* real = real_callback_.load(std::memory_order_acquire))
      real->SetRetransmissionMode(retransmission_mode);
  }

 private:
  void Apply(webrtc::FecProtectionParams& params) const {
    if (options_.has_fec_rate)
      params.fec_rate = options_.fec_rate;
    if (options_.has_mask_type)
      params.fec_mask_type = options_.mask_type;
    if (options_.has_max_frames)
      params.max_fec_frames = options_.max_frames;
  }

  const FecOverrideOptions options_;
  std::atomic<webrtc::VCMProtectionCallback*> real_callback_{nullptr};
};

// FecController delegating to FecControllerDefault with the override shim
// spliced into the protection callback path. Unrelated to webrtc's
// FecControllerOverride, which is the encoder-side enable/disable API.
class FecOverrideController : public webrtc::FecController {
 public:
  FecOverrideController(const webrtc::Environment& env,
                        FecOverrideOptions options)
      : shim_(options),
        inner_(std::make_unique<webrtc::FecControllerDefault>(env)) {}

  void SetProtectionCallback(
      webrtc::VCMProtectionCallback* protection_callback) override {
    shim_.SetRealCallback(protection_callback);
    inner_->SetProtectionCallback(protection_callback ? &shim_ : nullptr);
  }

  void SetProtectionMethod(bool enable_fec, bool enable_nack) override {
    inner_->SetProtectionMethod(enable_fec, enable_nack);
  }

  void SetEncodingData(size_t width,
                       size_t height,
                       size_t num_temporal_layers,
                       size_t max_payload_size) override {
    inner_->SetEncodingData(width, height, num_temporal_layers,
                            max_payload_size);
  }

  uint32_t UpdateFecRates(uint32_t estimated_bitrate_bps,
                          int actual_framerate,
                          uint8_t fraction_lost,
                          std::vector<bool> loss_mask_vector,
                          int64_t round_trip_time_ms) override {
    return inner_->UpdateFecRates(estimated_bitrate_bps, actual_framerate,
                                  fraction_lost, std::move(loss_mask_vector),
                                  round_trip_time_ms);
  }

  void UpdateWithEncodedData(
      size_t encoded_image_length,
      webrtc::VideoFrameType encoded_image_frametype) override {
    inner_->UpdateWithEncodedData(encoded_image_length,
                                  encoded_image_frametype);
  }

  bool UseLossVectorMask() override { return inner_->UseLossVectorMask(); }

 private:
  // shim_ must outlive inner_ during destruction (declared first).
  FecOverrideProtectionCallback shim_;
  std::unique_ptr<webrtc::FecControllerDefault> inner_;
};

class FecOverrideControllerFactory
    : public webrtc::FecControllerFactoryInterface {
 public:
  std::unique_ptr<webrtc::FecController> CreateFecController(
      const webrtc::Environment& env) override {
    // Snapshot the config per controller (one per video send stream).
    return std::make_unique<FecOverrideController>(env, GlobalFecOverride());
  }
};

}  // namespace

void SetGlobalFecOverride(const FecOverrideOptions& options) {
  {
    webrtc::MutexLock lock(&GlobalFecOverrideMutex());
    GlobalFecOverrideStorage() = options;
  }
  RTC_LOG(LS_INFO) << "FEC override configured: fec_rate="
                   << (options.has_fec_rate ? options.fec_rate : -1)
                   << " mask_type="
                   << (options.has_mask_type ? options.mask_type : -1)
                   << " max_frames="
                   << (options.has_max_frames ? options.max_frames : -1);
}

std::unique_ptr<webrtc::FecControllerFactoryInterface>
MaybeCreateFecControllerFactory() {
  if (!GlobalFecOverride().any())
    return nullptr;
  return std::make_unique<FecOverrideControllerFactory>();
}

}  // namespace livekit_ffi
