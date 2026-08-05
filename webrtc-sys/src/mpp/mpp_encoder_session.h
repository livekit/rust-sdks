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

#ifndef MPP_ENCODER_SESSION_H_
#define MPP_ENCODER_SESSION_H_

#include <cstddef>

#include <rockchip/mpp_buffer.h>
#include <rockchip/rk_mpi.h>
#include <rockchip/rk_venc_cfg.h>

namespace webrtc {

// Owns the MPP context, configuration, and reusable encoder buffers.
class MppEncoderSession final {
 public:
  MppEncoderSession() = default;
  ~MppEncoderSession();

  MppEncoderSession(const MppEncoderSession&) = delete;
  MppEncoderSession& operator=(const MppEncoderSession&) = delete;

  MPP_RET Initialize(MppCodingType codec_type);
  MPP_RET InitializeConfig();
  MPP_RET AllocateBuffers(size_t frame_size, size_t packet_size);
  void Reset();

  MppCtx context() const { return context_; }
  MppApi* api() const { return api_; }
  MppEncCfg config() const { return config_; }
  MppBuffer frame_buffer() const { return frame_buffer_; }
  MppBuffer packet_buffer() const { return packet_buffer_; }

 private:
  void ReleaseBuffers();

  MppCtx context_ = nullptr;
  MppApi* api_ = nullptr;
  MppEncCfg config_ = nullptr;
  MppBufferGroup buffer_group_ = nullptr;
  MppBuffer frame_buffer_ = nullptr;
  MppBuffer packet_buffer_ = nullptr;
};

}  // namespace webrtc

#endif  // MPP_ENCODER_SESSION_H_
