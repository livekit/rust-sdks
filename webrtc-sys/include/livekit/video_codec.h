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

#pragma once

#include <atomic>
#include <cstdint>
#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/task_queue/task_queue_base.h"
#include "api/video_codecs/video_codec.h"
#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_encoder.h"
#include "livekit/video_frame.h"
#include "livekit/video_frame_buffer.h"
#include "livekit/video_track.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit_ffi {
class NativeVideoEncoder;
class NativeVideoDecoder;
class VideoEncoderFactory;
class VideoDecoderFactory;
}  // namespace livekit_ffi
#include "webrtc-sys/src/video_codec.rs.h"

namespace livekit_ffi {

// A standalone video encoder built from the same factory the
// PeerConnectionFactory uses (and therefore honoring VideoEncoderBackend).
//
// Threading: every call into the wrapped webrtc::VideoEncoder (InitEncode,
// Encode, SetRates, Release, destruction) runs on `queue_`, a task queue owned
// by this object. Encoded output is delivered to the Rust observer from
// whichever thread the encoder produces it on (the queue for software
// encoders, a platform thread for hardware ones).
//
// Teardown: the destructor releases and destroys the encoder on `queue_`,
// waits for that to finish, then deletes the queue. Output arriving after that
// point is dropped, so the observer never outlives this object's use of it.
class NativeVideoEncoder {
 public:
  explicit NativeVideoEncoder(rust::Box<VideoEncoderObserver> observer);
  ~NativeVideoEncoder();

  NativeVideoEncoder(const NativeVideoEncoder&) = delete;
  NativeVideoEncoder& operator=(const NativeVideoEncoder&) = delete;

  // Creates and initializes the encoder. Blocks until initialization on the
  // encoder queue completes. Returns an empty string on success.
  std::string Initialize(const VideoEncoderSettings& settings);

  VideoEncodeResult encode(const VideoFrame& frame,
                           uint32_t rtp_timestamp) const;
  void request_keyframe(int32_t layer) const;
  void set_rates(rust::Slice<const uint32_t> layer_bitrates_bps,
                 double framerate_fps) const;
  void set_layer_active(uint32_t layer, bool active) const;
  VideoCodecInfo info() const;

 private:
  class Callback;

  // Run on `queue_`.
  void ApplyRatesOnQueue() const;
  void EncodeOnQueue(const webrtc::VideoFrame& frame,
                     uint32_t rtp_timestamp) const;

  webrtc::Environment env_;
  std::shared_ptr<VideoEncoderFactory> factory_;
  std::unique_ptr<Callback> callback_;
  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> queue_;

  // Owned and used only on `queue_` once Initialize returned.
  std::unique_ptr<webrtc::VideoEncoder> encoder_;
  webrtc::VideoCodec codec_;
  int num_streams_ = 1;
  int num_temporal_layers_ = 1;
  int width_ = 0;
  int height_ = 0;

  mutable std::atomic<int> pending_encodes_{0};
  mutable std::atomic<int> last_error_{0};
  // Bit `i` requests a keyframe on stream `i` with the next encoded frame.
  // Shared with `callback_`, which raises bits for output it had to drop.
  std::shared_ptr<std::atomic<uint32_t>> keyframe_requests_;

  mutable webrtc::Mutex rates_mutex_;
  mutable std::vector<uint32_t> layer_bitrates_bps_;
  mutable std::vector<bool> layer_active_;
  mutable double framerate_fps_ = 30.0;
};

std::unique_ptr<NativeVideoEncoder> create_video_encoder(
    const VideoEncoderSettings& settings,
    rust::Box<VideoEncoderObserver> observer);

// A standalone video decoder built from the same factory the
// PeerConnectionFactory uses.
//
// Threading and teardown mirror NativeVideoEncoder: the decoder lives on
// `queue_`, decoded frames reach the Rust observer from the decoding thread,
// and the destructor tears the decoder down on the queue before returning.
//
// Keyframe handling: a delta frame is only decoded when every frame since
// the last keyframe was decoded. Before the first keyframe, after a decode
// error and after an input frame was dropped because the decoder fell
// behind, delta frames are rejected with KeyframeRequired until the caller
// supplies a keyframe.
class NativeVideoDecoder {
 public:
  explicit NativeVideoDecoder(rust::Box<VideoDecoderObserver> observer);
  ~NativeVideoDecoder();

  NativeVideoDecoder(const NativeVideoDecoder&) = delete;
  NativeVideoDecoder& operator=(const NativeVideoDecoder&) = delete;

  // With `prefer_software`, platform (hardware) decoder factories are
  // skipped.
  std::string Initialize(EncodedVideoCodec codec, bool prefer_software);

  VideoDecodeStatus decode(rust::Slice<const uint8_t> data,
                           bool is_keyframe,
                           uint32_t rtp_timestamp,
                           uint32_t width,
                           uint32_t height) const;
  VideoCodecInfo info() const;
  uint64_t decode_error_count() const { return decode_errors_.load(); }

 private:
  class Callback;

  webrtc::Environment env_;
  std::shared_ptr<VideoDecoderFactory> factory_;
  std::unique_ptr<Callback> callback_;
  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> queue_;

  // Run on `queue_`.
  void DecodeOnQueue(const webrtc::EncodedImage& image,
                     uint64_t generation) const;

  // Owned and used only on `queue_` once Initialize returned.
  std::unique_ptr<webrtc::VideoDecoder> decoder_;
  mutable bool queue_needs_keyframe_ = true;

  mutable std::atomic<int> pending_decodes_{0};
  // Submission-side keyframe gate, see the class comment.
  mutable std::atomic<bool> needs_keyframe_{true};
  // Incremented for every submitted keyframe, so a decode error only closes
  // the gate when no newer keyframe has been submitted since.
  mutable std::atomic<uint64_t> keyframe_generation_{0};
  mutable std::atomic<uint64_t> decode_errors_{0};
};

std::unique_ptr<NativeVideoDecoder> create_video_decoder(
    EncodedVideoCodec codec,
    bool prefer_software,
    rust::Box<VideoDecoderObserver> observer);

// Describes a pre-encoded access unit carried by a native frame buffer, as
// produced by VideoTrackSource::capture_encoded_frame.
EncodedBufferInfo encoded_buffer_info(const VideoFrameBuffer& buffer);
rust::Slice<const uint8_t> encoded_buffer_payload(
    const VideoFrameBuffer& buffer);

// WebRTC's send-side processing of an H.264 keyframe: SPSs get bitstream
// restrictions (receivers rewrite them otherwise, which breaks frame
// encryption) and access unit delimiters are dropped.
rust::Vec<uint8_t> rewrite_h264_keyframe(rust::Slice<const uint8_t> data);

// Number of live NativeVideoEncoder / NativeVideoDecoder instances. Lets
// lifecycle tests observe that native codecs are actually destroyed.
int64_t live_video_codec_count();

}  // namespace livekit_ffi
