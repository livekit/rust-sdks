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

#include <memory>

#include "api/fec_controller.h"
#include "modules/include/module_fec_types.h"

namespace livekit_ffi {

// Process-global overrides applied to the FEC protection parameters computed
// by libwebrtc's default FEC controller. has_* flags follow the same
// optional-field convention as the cxx bridge structs.
struct FecOverrideOptions {
  bool has_fec_rate = false;
  int fec_rate = 0;  // 0-255, applied to both delta and key frame params
  bool has_mask_type = false;
  webrtc::FecMaskType mask_type = webrtc::kFecMaskRandom;
  bool has_max_frames = false;
  int max_frames = 0;

  bool any() const { return has_fec_rate || has_mask_type || has_max_frames; }
};

// Stores the process-global override config. Must be called before the first
// PeerConnectionFactory is created; the factory only injects the override
// controller when overrides are present at construction time.
void SetGlobalFecOverride(const FecOverrideOptions& options);

// Returns a factory wrapping webrtc::FecControllerDefault that applies the
// global overrides, or nullptr when no overrides are configured (keeping
// webrtc's default adaptive FEC behavior).
std::unique_ptr<webrtc::FecControllerFactoryInterface>
MaybeCreateFecControllerFactory();

}  // namespace livekit_ffi
