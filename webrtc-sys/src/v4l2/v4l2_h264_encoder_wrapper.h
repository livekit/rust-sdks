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

#include <cstddef>
#include <cstdint>
#include <deque>
#include <string>

#include "api/scoped_refptr.h"
#include "api/video/encoded_image.h"
#include "api/video/video_frame_buffer.h"

namespace livekit_ffi {

// How the OUTPUT (raw YUV) queue is fed.
//
//   Mmap    -- driver-allocated buffers, user-space writes via memcpy.
//              Always works; one copy per frame.
//   UserPtr -- user-space pointer is passed to the driver. Eliminates the
//              CopyI420ToOutputBuffer memcpy when the source I420 planes
//              are already contiguous in memory. Falls back transparently
//              to memcpy when planes are not contiguous.
//   Dmabuf  -- the OUTPUT queue imports DMABUF file descriptors. True
//              zero-copy when the source produces DMABUF-backed frames
//              (e.g. libcamera on Pi 4). Only `EncodeDmabuf` may be called
//              in this mode.
enum class OutputBufferMode {
  Mmap,
  UserPtr,
  Dmabuf,
};

struct EncodedFrame {
  webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface> bitstream;
  uint32_t rtp_timestamp = 0;
  bool key_frame = false;
};

struct EncodeResult {
  enum class Status {
    Ok,
    NoOutput,
    Error,
  };

  Status status = Status::Error;
  EncodedFrame frame;
};

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
  // |mode| selects the OUTPUT-queue memory model; see `OutputBufferMode`.
  // |input_fourcc| is the V4L2 pixelformat of the input (e.g.
  // V4L2_PIX_FMT_YUV420 or V4L2_PIX_FMT_NV12). Defaults to YUV420.
  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int keyframe_interval,
                  int framerate,
                  OutputBufferMode mode = OutputBufferMode::Mmap,
                  uint32_t input_fourcc = 0x32315559,  // V4L2_PIX_FMT_YUV420 = "YU12"
                  int input_stride = 0,
                  const std::string& device_path = "");

  // Encode a single planar YUV frame.
  // In `Mmap` mode the planes are memcpy'd into a driver-allocated buffer.
  // In `UserPtr` mode the buffer is fed to the encoder via userspace pointer
  // when the planes are contiguous; otherwise it falls back to memcpy.
  // Not valid in `Dmabuf` mode.
  //
  // On success returns an encoded frame with the H.264 bitstream and
  // metadata copied from the originating input frame. May return NoOutput
  // when the stateful encoder has accepted input but has not produced a
  // completed coded frame yet.
  EncodeResult Encode(
      const uint8_t* y,
      const uint8_t* u,
      const uint8_t* v,
      int stride_y,
      int stride_u,
      int stride_v,
      bool force_idr,
      uint32_t rtp_timestamp);

  // Encode a single DMABUF-backed YUV frame. Only valid in `Dmabuf` mode.
  // The fd is borrowed. `retained_input_buffer`, when provided, is held until
  // V4L2 dequeues the submitted OUTPUT buffer so the underlying DMABUF cannot
  // be recycled while the hardware is still reading it.
  //
  // `offset` is the byte offset into the dmabuf where the frame data
  // begins, `length` is the total size of the YUV frame in bytes (use 0 to
  // request the encoder's configured `sizeimage`).
  EncodeResult EncodeDmabuf(
      int dmabuf_fd,
      size_t offset,
      size_t length,
      bool force_idr,
      uint32_t rtp_timestamp,
      webrtc::scoped_refptr<webrtc::VideoFrameBuffer> retained_input_buffer);

  // Update bitrate (bps) and framerate (fps) at runtime.
  void UpdateRates(int framerate, int bitrate);

  bool IsInitialized() const { return initialized_; }
  OutputBufferMode mode() const { return mode_; }
  // Negotiated OUTPUT-queue pixel format after initialization.
  uint32_t output_fourcc() const { return input_fourcc_; }
  int output_stride() const { return output_stride_; }

  // Stop streaming and release all V4L2 resources.
  void Destroy();

  // ioctl() wrapper with automatic EINTR retry. Exposed for use by
  // helper functions in the implementation file.
  static int Xioctl(int fd, unsigned long ctl, void* arg);

 private:
  // Number of MMAP buffers to request for each queue.
  static constexpr int kNumOutputBuffers = 4;
  static constexpr int kNumCaptureBuffers = 4;
  // A working M2M pipeline needs at least 2 buffers per queue so the
  // encoder can have one queued while user-space holds another.
  static constexpr int kMinBuffersPerQueue = 2;

  // Feed black frames through the encoder to prime its internal pipeline.
  // The bcm2835 encoder needs several frames before it produces valid output.
  void PrimeEncoderPipeline();

  int AcquireOutputBuffer(int timeout_ms);
  void DrainReadyOutputBuffers();
  void DrainReadyCaptureBuffers();
  bool QueueCaptureBuffer(int index);
  EncodeResult WaitForEncodedFrame(int timeout_ms);
  bool WaitForOutputBuffer(int index, int timeout_ms);

  // Copy an I420 frame into the mmap'd OUTPUT buffer at |index|.
  void CopyI420ToOutputBuffer(int index,
                              const uint8_t* y,
                              const uint8_t* u,
                              const uint8_t* v,
                              int stride_y,
                              int stride_u,
                              int stride_v);

  // Submit OUTPUT buffer at `buf_index` and wait briefly for an encoded
  // CAPTURE buffer. Used by both `Encode` and `EncodeDmabuf`.
  EncodeResult RunEncode(
      int buf_index,
      bool force_idr,
      const uint8_t* userptr,
      int dmabuf_fd,
      size_t offset,
      size_t length,
      uint32_t rtp_timestamp,
      webrtc::scoped_refptr<webrtc::VideoFrameBuffer> retained_input_buffer,
      int encoded_timeout_ms,
      bool wait_for_output_buffer);

  struct PendingFrame {
    uint64_t v4l2_timestamp_us = 0;
    uint32_t rtp_timestamp = 0;
    bool key_frame = false;
    bool requires_parameter_sets = false;
  };

  // --- State ---
  bool initialized_ = false;
  int fd_ = -1;
  int width_ = 0;
  int height_ = 0;
  int framerate_ = 30;
  int bitrate_ = 0;
  int frame_size_ = 0;  // configured sizeimage for OUTPUT (single plane)
  int output_stride_ = 0;
  int output_chroma_stride_ = 0;
  int output_luma_height_ = 0;
  int output_chroma_height_ = 0;
  int capture_buffer_size_ = 0;
  OutputBufferMode mode_ = OutputBufferMode::Mmap;
  uint32_t input_fourcc_ = 0;

  // MMAP'd buffer descriptor (one per slot in each queue).
  struct MmapBuffer {
    void* start = nullptr;
    size_t length = 0;
  };

  // OUTPUT queue buffers (raw YUV frames fed into the encoder).
  // Only populated when |mode_| == Mmap.
  MmapBuffer output_buffers_[kNumOutputBuffers];
  int num_output_buffers_ = 0;
  bool output_buffer_queued_[kNumOutputBuffers] = {};
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
      retained_input_buffers_[kNumOutputBuffers];

  // CAPTURE queue buffers (encoded H.264 bitstream from the encoder).
  MmapBuffer capture_buffers_[kNumCaptureBuffers];
  int num_capture_buffers_ = 0;

  // Round-robin index for the next OUTPUT buffer to use.
  int next_output_index_ = 0;

  std::deque<PendingFrame> pending_frames_;
  std::deque<EncodedFrame> ready_frames_;
  uint64_t next_v4l2_timestamp_us_ = 1;
  bool force_next_keyframe_ = false;
  bool require_next_keyframe_parameter_sets_ = true;
};

}  // namespace livekit_ffi

#endif  // V4L2_H264_ENCODER_WRAPPER_H_
