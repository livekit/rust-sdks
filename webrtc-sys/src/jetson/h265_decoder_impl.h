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

#ifndef WEBRTC_JETSON_H265_DECODER_IMPL_H_
#define WEBRTC_JETSON_H265_DECODER_IMPL_H_

#include <atomic>
#include <cstdint>

#include "api/video_codecs/video_decoder.h"
#include "common_video/include/video_frame_buffer_pool.h"
#include "jetson_mmapi_decoder.h"

namespace webrtc {

// H.265 decoding on the Jetson NVDEC engine.
//
// Decoding is asynchronous: Decode() returns once the access unit is queued,
// and pictures reach the registered DecodedImageCallback from the decoder's
// capture thread. Release() joins that thread before returning.
class JetsonH265DecoderImpl : public VideoDecoder {
 public:
  JetsonH265DecoderImpl();
  JetsonH265DecoderImpl(const JetsonH265DecoderImpl&) = delete;
  JetsonH265DecoderImpl& operator=(const JetsonH265DecoderImpl&) = delete;
  ~JetsonH265DecoderImpl() override;

  bool Configure(const Settings& settings) override;
  int32_t Decode(const EncodedImage& input_image,
                 bool missing_frames,
                 int64_t render_time_ms) override;
  int32_t RegisterDecodeCompleteCallback(
      DecodedImageCallback* callback) override;
  int32_t Release() override;
  DecoderInfo GetDecoderInfo() const override;

 private:
  bool StartDecoder();
  // Runs on the decoder's capture thread.
  void OnFrame(const livekit::JetsonDecodedFrame& frame);

  livekit::JetsonMmapiDecoder decoder_;
  std::atomic<DecodedImageCallback*> decoded_complete_callback_{nullptr};
  // Capture thread only, apart from Release() after the thread is joined.
  VideoFrameBufferPool buffer_pool_;
  // A (re)started decoder cannot begin on a delta frame.
  bool awaiting_keyframe_ = true;
};

}  // namespace webrtc

#endif  // WEBRTC_JETSON_H265_DECODER_IMPL_H_
