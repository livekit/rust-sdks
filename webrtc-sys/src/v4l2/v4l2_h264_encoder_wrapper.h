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

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <functional>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>

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

// Asynchronous callback invoked from the wrapper's poll thread for every
// encoded frame the V4L2 hardware emits. Mirroring rpicam-apps's
// `OutputReadyCallback`, this lets the caller deliver encoded data to
// the WebRTC pipeline as soon as the encoder finishes a frame -- without
// waiting for the next `Encode()` call to drain it. The callback runs on
// the wrapper's poll thread and must be reentrant-safe but need not be
// thread-safe with itself (calls are serialized on the poll thread).
using EncodedFrameCallback = std::function<void(EncodedFrame)>;

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
  //
  // |input_colorspace_v4l2| is the V4L2 `v4l2_colorspace` value of the
  // OUTPUT-queue (input) frames (e.g. `V4L2_COLORSPACE_REC709 == 3`,
  // `V4L2_COLORSPACE_SMPTE170M == 1`). Pass 0 (`V4L2_COLORSPACE_DEFAULT`)
  // to let the wrapper pick `SMPTE170M` for SD and `REC709` for HD --
  // matching what `rpicam-apps` does when the producer doesn't specify.
  bool Initialize(int width,
                  int height,
                  int bitrate,
                  int keyframe_interval,
                  int framerate,
                  OutputBufferMode mode = OutputBufferMode::Mmap,
                  uint32_t input_fourcc = 0x32315559,  // V4L2_PIX_FMT_YUV420 = "YU12"
                  int input_stride = 0,
                  uint32_t input_colorspace_v4l2 = 0,
                  const std::string& device_path = "");

  // Install (or clear, with `nullptr`) the asynchronous encoded-frame
  // callback. Once set, the wrapper's poll thread invokes the callback
  // for every encoded frame the V4L2 hardware emits, instead of buffering
  // them in `ready_frames_` to be drained by `Encode()/EncodeDmabuf()`'s
  // synchronous wait. This matches rpicam-apps's separate `outputThread`
  // model and avoids OnEncodedImage latency that would otherwise grow
  // with the encoder's CAPTURE-queue depth.
  //
  // Safe to call before or after `Initialize()`. The wrapper's
  // destructor / `Destroy()` joins the poll thread before returning, so
  // the callback is guaranteed not to fire after `Destroy()` returns.
  void SetEncodedFrameCallback(EncodedFrameCallback callback);

  // Encode a single planar YUV frame.
  // In `Mmap` mode the planes are memcpy'd into a driver-allocated buffer.
  // In `UserPtr` mode the buffer is fed to the encoder via userspace pointer
  // when the planes are contiguous; otherwise it falls back to memcpy.
  // Not valid in `Dmabuf` mode.
  //
  // `capture_timestamp_us` is forwarded verbatim to V4L2 as the OUTPUT
  // buffer timestamp (matching rpicam-apps's behaviour) and used to
  // correlate the emitted CAPTURE buffer back to its source frame. It
  // should be a monotonically increasing value (e.g. the input frame's
  // capture-time microseconds) so the bcm2835-codec rate controller sees
  // realistic frame spacing.
  //
  // On success returns an encoded frame with the H.264 bitstream and
  // metadata copied from the originating input frame. When an
  // asynchronous `EncodedFrameCallback` is installed, this always
  // returns NoOutput on success and the encoded frame is delivered via
  // the callback instead. May also return NoOutput when the stateful
  // encoder has accepted input but has not produced a completed coded
  // frame yet.
  EncodeResult Encode(
      const uint8_t* y,
      const uint8_t* u,
      const uint8_t* v,
      int stride_y,
      int stride_u,
      int stride_v,
      bool force_idr,
      uint32_t rtp_timestamp,
      int64_t capture_timestamp_us);

  // Encode a single DMABUF-backed YUV frame. Only valid in `Dmabuf` mode.
  // The fd is borrowed. `retained_input_buffer`, when provided, is held until
  // V4L2 dequeues the submitted OUTPUT buffer so the underlying DMABUF cannot
  // be recycled while the hardware is still reading it.
  //
  // `offset` is the byte offset into the dmabuf where the frame data
  // begins, `length` is the total size of the YUV frame in bytes (use 0 to
  // request the encoder's configured `sizeimage`).
  //
  // See `Encode()` for the meaning of `capture_timestamp_us`.
  EncodeResult EncodeDmabuf(
      int dmabuf_fd,
      size_t offset,
      size_t length,
      bool force_idr,
      uint32_t rtp_timestamp,
      int64_t capture_timestamp_us,
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
  // Number of buffers to request for each queue. Sized to match
  // rpicam-apps: 6 OUTPUT buffers so we always have at least as many as
  // the libcamera capture queue (we want to be able to immediately
  // import a new DMABUF the moment libcamera produces one), and 12
  // CAPTURE buffers so the encoder has ample room to keep producing
  // bitstream while user-space is draining it.
  static constexpr int kNumOutputBuffers = 6;
  static constexpr int kNumCaptureBuffers = 12;
  // A working M2M pipeline needs at least 2 buffers per queue so the
  // encoder can have one queued while user-space holds another.
  static constexpr int kMinBuffersPerQueue = 2;

  // Feed black frames through the encoder to prime its internal pipeline.
  // The bcm2835 encoder needs several frames before it produces valid output.
  void PrimeEncoderPipeline();

  // Poll-thread loop. Blocks on poll(fd, POLLIN, ...) and drains both
  // OUTPUT and CAPTURE queues whenever the encoder signals readiness.
  // Spawned in Initialize() after STREAMON and joined in Destroy().
  void PollThreadLoop();

  // Owns DQBUF / queue-state mutation. Called only from PollThreadLoop()
  // after poll() reports POLLIN. They acquire `mutex_` internally and
  // notify the appropriate condvar after updating shared state, so the
  // WebRTC encoder thread can wake without hammering the V4L2 fd.
  void DrainReadyOutputBuffers();
  void DrainReadyCaptureBuffers();

  int AcquireOutputBuffer(int timeout_ms);
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

  // Emit a single-line periodic snapshot of throughput counters so an
  // operator can see in real time where the encoder is spending time
  // (submission rate vs. hardware completion rate vs. queue waits).
  // Called from the poll thread once per second; resets the counters
  // after logging.
  void MaybeLogThroughputStats();

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
      int64_t capture_timestamp_us,
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
  // Counter for fabricating monotonically-increasing V4L2 timestamps when
  // the caller passes in 0 (or a non-monotonic value). Real timestamps
  // from the producer are preferred; this is a defensive fallback.
  uint64_t next_synthetic_v4l2_timestamp_us_ = 1;
  bool force_next_keyframe_ = false;
  bool require_next_keyframe_parameter_sets_ = true;
  EncodedFrameCallback encoded_frame_callback_;

  // --- Poll-thread synchronisation ---
  //
  // The poll thread owns DQBUF on both queues and updates
  // `output_buffer_queued_`, `retained_input_buffers_`, `pending_frames_`,
  // `ready_frames_`, `force_next_keyframe_`, and
  // `require_next_keyframe_parameter_sets_` under `mutex_`. The encoder
  // thread (calls into Encode/EncodeDmabuf) updates the same fields while
  // QBUF-ing under the same mutex, then waits on the appropriate condvar
  // for state changes instead of polling the V4L2 fd directly.
  std::mutex mutex_;
  std::condition_variable output_buffer_cv_;
  std::condition_variable encoded_frame_cv_;
  std::thread poll_thread_;
  std::atomic<bool> abort_poll_{false};

  // --- Lightweight throughput diagnostics ---
  //
  // All counters are updated under `mutex_` (or with relaxed atomics
  // where they're only read by the poll thread) and reset every time
  // the poll thread emits a periodic stats line. This is the minimum
  // instrumentation needed to localise an encoder bottleneck (slow
  // hardware vs. starved input vs. wedged output queue) without
  // shipping per-frame logs to production builds.
  struct ThroughputStats {
    uint32_t encode_calls = 0;       // entries to Encode/EncodeDmabuf
    uint32_t output_dequeued = 0;    // OUTPUT (input-consumed) DQBUFs
    uint32_t capture_dequeued = 0;   // CAPTURE (encoded) DQBUFs
    uint32_t acquire_calls = 0;      // calls to AcquireOutputBuffer
    uint32_t acquire_waited = 0;     // those that hit the cv (queue full)
    uint64_t acquire_wait_total_us = 0;
    uint64_t acquire_wait_max_us = 0;
    uint64_t encode_latency_total_us = 0;  // QBUF -> matched DQBUF
    uint32_t encode_latency_count = 0;
    uint64_t encode_latency_max_us = 0;
  };
  ThroughputStats stats_;
  std::chrono::steady_clock::time_point stats_last_log_ =
      std::chrono::steady_clock::now();
  // Maps v4l2 buffer timestamp -> steady_clock time of QBUF, used to
  // compute per-frame encode latency in DrainReadyCaptureBuffers.
  // Bounded by the number of in-flight frames (<= kNumOutputBuffers +
  // kNumCaptureBuffers); cleaned up as frames complete.
  std::unordered_map<uint64_t, std::chrono::steady_clock::time_point>
      qbuf_steady_time_;
};

}  // namespace livekit_ffi

#endif  // V4L2_H264_ENCODER_WRAPPER_H_
