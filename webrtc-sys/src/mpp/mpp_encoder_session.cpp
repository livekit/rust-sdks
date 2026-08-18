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

#include "mpp_encoder_session.h"

namespace webrtc {

MppEncoderSession::~MppEncoderSession() {
  Reset();
}

MPP_RET MppEncoderSession::Initialize(MppCodingType codec_type) {
  Reset();

  MPP_RET result = mpp_create(&context_, &api_);
  if (result != MPP_OK) {
    Reset();
    return result;
  }

  result = mpp_init(context_, MPP_CTX_ENC, codec_type);
  if (result != MPP_OK) {
    Reset();
  }
  return result;
}

MPP_RET MppEncoderSession::InitializeConfig() {
  if (!context_ || !api_) {
    return MPP_NOK;
  }

  if (config_) {
    mpp_enc_cfg_deinit(config_);
    config_ = nullptr;
  }

  MPP_RET result = mpp_enc_cfg_init(&config_);
  if (result != MPP_OK) {
    config_ = nullptr;
    return result;
  }

  result = api_->control(context_, MPP_ENC_GET_CFG, config_);
  if (result != MPP_OK) {
    mpp_enc_cfg_deinit(config_);
    config_ = nullptr;
  }
  return result;
}

MPP_RET MppEncoderSession::AllocateBuffers(size_t frame_size,
                                           size_t packet_size) {
  ReleaseBuffers();

  MPP_RET result = mpp_buffer_group_get_internal(
      &buffer_group_, MPP_BUFFER_TYPE_DRM | MPP_BUFFER_FLAGS_CACHABLE);
  if (result != MPP_OK) {
    if (buffer_group_) {
      mpp_buffer_group_put(buffer_group_);
      buffer_group_ = nullptr;
    }
    result = mpp_buffer_group_get_internal(
        &buffer_group_, MPP_BUFFER_TYPE_ION | MPP_BUFFER_FLAGS_CACHABLE);
  }
  if (result != MPP_OK) {
    ReleaseBuffers();
    return result;
  }

  result = mpp_buffer_get(buffer_group_, &frame_buffer_, frame_size);
  if (result != MPP_OK) {
    ReleaseBuffers();
    return result;
  }

  result = mpp_buffer_get(buffer_group_, &packet_buffer_, packet_size);
  if (result != MPP_OK) {
    ReleaseBuffers();
  }
  return result;
}

void MppEncoderSession::Reset() {
  ReleaseBuffers();

  if (config_) {
    mpp_enc_cfg_deinit(config_);
    config_ = nullptr;
  }
  if (context_) {
    mpp_destroy(context_);
    context_ = nullptr;
    api_ = nullptr;
  }
}

void MppEncoderSession::ReleaseBuffers() {
  if (packet_buffer_) {
    mpp_buffer_put(packet_buffer_);
    packet_buffer_ = nullptr;
  }
  if (frame_buffer_) {
    mpp_buffer_put(frame_buffer_);
    frame_buffer_ = nullptr;
  }
  if (buffer_group_) {
    mpp_buffer_group_put(buffer_group_);
    buffer_group_ = nullptr;
  }
}

}  // namespace webrtc
