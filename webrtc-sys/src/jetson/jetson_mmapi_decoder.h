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

#ifndef LIVEKIT_JETSON_MMAPI_DECODER_H_
#define LIVEKIT_JETSON_MMAPI_DECODER_H_

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <thread>

#include "jetson_mmapi_encoder.h"

class NvVideoDecoder;
struct NvBufSurface;

namespace livekit {

// A decoded I420 picture in CPU memory. The planes are only valid for the
// duration of the JetsonMmapiDecoder::FrameCallback call that receives it.
struct JetsonDecodedFrame {
  const uint8_t* data_y = nullptr;
  int stride_y = 0;
  const uint8_t* data_u = nullptr;
  int stride_u = 0;
  const uint8_t* data_v = nullptr;
  int stride_v = 0;
  int width = 0;
  int height = 0;
  uint32_t rtp_timestamp = 0;
};

// Hardware video decoder on the Jetson Multimedia API (NVDEC).
//
// Decode() queues one complete access unit and returns without waiting for
// it to be decoded. Pictures are delivered on a capture thread this object
// owns; Destroy() stops and joins that thread, so the callback never runs
// after Destroy() returns.
class JetsonMmapiDecoder {
 public:
  using FrameCallback = std::function<void(const JetsonDecodedFrame&)>;

  explicit JetsonMmapiDecoder(JetsonCodec codec);
  ~JetsonMmapiDecoder();

  JetsonMmapiDecoder(const JetsonMmapiDecoder&) = delete;
  JetsonMmapiDecoder& operator=(const JetsonMmapiDecoder&) = delete;

  // Whether this host has an NVDEC device and libraries that accept `codec`.
  static bool IsCodecSupported(JetsonCodec codec);

  bool Initialize(FrameCallback callback);
  void Destroy();
  bool IsInitialized() const { return decoder_ != nullptr; }

  // Queues one complete access unit. Fails when the decoder hit an error or
  // no input buffer frees up in time; the caller should request a keyframe.
  bool Decode(const uint8_t* data, size_t size, uint32_t rtp_timestamp);

  bool HasFailed() const { return failed_.load(); }

 private:
  enum class PictureResult { kDelivered, kNone, kFailed };

  void CaptureLoop();
  bool HandleCaptureEvents();
  bool SetupCapturePlane();
  void ReleaseCapturePlane();
  // Dequeues and delivers one decoded picture, if any is ready.
  PictureResult DeliverNextPicture(bool requeue);
  bool DeliverFrame(int capture_fd, uint32_t rtp_timestamp);

  const JetsonCodec codec_;
  NvVideoDecoder* decoder_ = nullptr;
  FrameCallback callback_;

  std::thread capture_thread_;
  std::atomic<bool> stop_{false};
  std::atomic<bool> failed_{false};

  // Decode() thread only.
  uint32_t output_buffers_used_ = 0;

  // Capture thread only.
  bool capture_ready_ = false;
  int width_ = 0;
  int height_ = 0;
  int converted_fd_ = -1;
  NvBufSurface* converted_surface_ = nullptr;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_MMAPI_DECODER_H_
