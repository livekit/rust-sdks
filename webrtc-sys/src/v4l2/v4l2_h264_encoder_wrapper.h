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

#ifndef V4L2_H264_ENCODER_WRAPPER_H_
#define V4L2_H264_ENCODER_WRAPPER_H_

#include <cstdint>
#include <string>
#include <vector>

namespace livekit_ffi {

// Low-level wrapper around a V4L2 memory-to-memory (M2M) H.264 hardware
// encoder, as found on Raspberry Pi (bcm2835-codec).
//
// The V4L2 M2M model uses two buffer queues:
//   OUTPUT  queue  -- raw YUV frames fed *into* the encoder  (our input)
//   CAPTURE queue  -- encoded H.264 bitstream read *from* the encoder (our output)
//
// Typical lifecycle:
//   1. FindEncoderDevice()   -- locate a suitable /dev/videoN device
//   2. Initialize()          -- open device, configure format/controls, mmap buffers
//   3. Encode() in a loop    -- submit I420 frames, receive H.264 NALUs
//   4. Destroy()             -- stop streaming, unmap, close fd
class V4l2H264EncoderWrapper {
 public:
  V4l2H264EncoderWrapper();
  ~V4l2H264EncoderWrapper();

  // Probe /dev/video* for a V4L2 M2M device that supports H.264 encoding.
  // Returns the device path (e.g. "/dev/video11") or empty string if none found.
  static std::string FindEncoderDevice();

  // Initialize the encoder with the given parameters.
  // |device_path| may be empty, in which case FindEncoderDevice() is called.
  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int keyframe_interval,
                  int framerate,
                  const std::string& device_path = "");

  // Encode a single I420 frame.
  // |y|, |u|, |v| point to the respective planes with the given strides.
  // If |force_idr| is true, a keyframe is requested for this frame.
  // On success the encoded H.264 bitstream is written to |output|.
  bool Encode(const uint8_t* y,
              const uint8_t* u,
              const uint8_t* v,
              int stride_y,
              int stride_u,
              int stride_v,
              bool force_idr,
              std::vector<uint8_t>& output);

  // Update bitrate (bps) and framerate (fps) at runtime.
  void UpdateRates(int framerate, int bitrate);

  bool IsInitialized() const { return initialized_; }

  // Stop streaming and release all V4L2 resources.
  void Destroy();

 private:
  // Number of MMAP buffers to request for each queue.
  static constexpr int kNumOutputBuffers = 4;
  static constexpr int kNumCaptureBuffers = 4;

  // ioctl() wrapper with automatic EINTR retry.
  static int Xioctl(int fd, unsigned long ctl, void* arg);

  // Feed black frames through the encoder to prime its internal pipeline.
  // The bcm2835 encoder needs several frames before it produces valid output.
  void PrimeEncoderPipeline();

  // Copy an I420 frame into the mmap'd OUTPUT buffer at |index|.
  void CopyI420ToOutputBuffer(int index,
                              const uint8_t* y,
                              const uint8_t* u,
                              const uint8_t* v,
                              int stride_y,
                              int stride_u,
                              int stride_v);

  // --- State ---
  bool initialized_ = false;
  int fd_ = -1;
  int width_ = 0;
  int height_ = 0;
  int framerate_ = 30;

  // MMAP'd buffer descriptor (one per slot in each queue).
  struct MmapBuffer {
    void* start = nullptr;
    size_t length = 0;
  };

  // OUTPUT queue buffers (raw YUV frames fed into the encoder).
  MmapBuffer output_buffers_[kNumOutputBuffers];
  int num_output_buffers_ = 0;

  // CAPTURE queue buffers (encoded H.264 bitstream from the encoder).
  MmapBuffer capture_buffers_[kNumCaptureBuffers];
  int num_capture_buffers_ = 0;

  // Round-robin index for the next OUTPUT buffer to use.
  int next_output_index_ = 0;

  // Force the very first encoded frame to be an IDR keyframe so the
  // decoder starts with a clean reference.
  bool first_frame_ = true;
};

}  // namespace livekit_ffi

#endif  // V4L2_H264_ENCODER_WRAPPER_H_
