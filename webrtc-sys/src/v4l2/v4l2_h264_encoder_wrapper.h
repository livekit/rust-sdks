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

class V4l2H264EncoderWrapper {
 public:
  V4l2H264EncoderWrapper();
  ~V4l2H264EncoderWrapper();

  // Probe /dev/video* for a V4L2 M2M device that supports H.264 encoding.
  // Returns the device path (e.g. "/dev/video11") or empty string if none found.
  static std::string FindEncoderDevice();

  // Initialize the encoder with the given parameters.
  // device_path may be empty, in which case FindEncoderDevice() is called.
  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int keyframe_interval,
                  int framerate,
                  const std::string& device_path = "");

  // Encode a single I420 frame. Y/U/V point to the respective planes.
  // If forceIDR is true, a keyframe is requested for this frame.
  // The encoded H.264 bitstream is appended to |output|.
  bool Encode(const uint8_t* y,
              const uint8_t* u,
              const uint8_t* v,
              int stride_y,
              int stride_u,
              int stride_v,
              bool forceIDR,
              std::vector<uint8_t>& output);

  // Update bitrate and framerate at runtime.
  void UpdateRates(int framerate, int bitrate);

  bool IsInitialized() const { return initialized_; }

  // Release all resources.
  void Destroy();

 private:
  static constexpr int NUM_OUTPUT_BUFFERS = 4;
  static constexpr int NUM_CAPTURE_BUFFERS = 4;

  // Helper: ioctl with EINTR retry.
  static int Xioctl(int fd, unsigned long ctl, void* arg);

  // Copy an I420 frame into the mmap'd output buffer at |index|.
  void CopyI420ToOutputBuffer(int index,
                              const uint8_t* y,
                              const uint8_t* u,
                              const uint8_t* v,
                              int stride_y,
                              int stride_u,
                              int stride_v);

  bool initialized_ = false;
  int fd_ = -1;
  int width_ = 0;
  int height_ = 0;
  int framerate_ = 30;

  // Output (encoder input) buffers -- MMAP'd userspace pointers.
  struct MmapBuffer {
    void* start = nullptr;
    size_t length = 0;
  };
  MmapBuffer output_buffers_[NUM_OUTPUT_BUFFERS];
  int num_output_buffers_ = 0;

  // Capture (encoder output) buffers -- MMAP'd.
  MmapBuffer capture_buffers_[NUM_CAPTURE_BUFFERS];
  int num_capture_buffers_ = 0;

  // Index of the next output buffer to use (round-robin).
  int next_output_index_ = 0;
};

}  // namespace livekit_ffi

#endif  // V4L2_H264_ENCODER_WRAPPER_H_
