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

#ifndef WEBRTC_SYS_MPP_CONTEXT_H
#define WEBRTC_SYS_MPP_CONTEXT_H

#include <rockchip/rk_mpi.h>
#include <rockchip/mpp_buffer.h>
#include <rockchip/mpp_frame.h>
#include <rockchip/mpp_packet.h>
#include <rockchip/rk_venc_cfg.h>

namespace livekit_ffi {

class MppContext {
 public:
  MppContext() = default;
  ~MppContext() = default;

  /// Check if the Rockchip MPP library is available on this system.
  static bool IsAvailable();

  /// Get the singleton instance.
  static MppContext* GetInstance();

 private:
  static bool LoadLibrary();
};

}  // namespace livekit_ffi

#endif  // WEBRTC_SYS_MPP_CONTEXT_H
