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

#include "livekit_rtc/include/capi.h"
#include "livekit_rtc/video_frame_buffer.h"

#include "api/scoped_refptr.h"
#include "api/video/video_frame.h"
#include "rtc_base/checks.h"

namespace livekit_ffi {

class VideoFrame : public webrtc::RefCountInterface {
 public:
  explicit VideoFrame(const webrtc::VideoFrame& frame);
  ~VideoFrame();

  unsigned int width() const;
  unsigned int height() const;
  uint32_t size() const;
  uint16_t id() const;
  int64_t timestamp_us() const;
  int64_t ntp_time_ms() const;
  uint32_t timestamp() const;

  lkVideoRotation rotation() const;
  webrtc::scoped_refptr<VideoFrameBuffer> video_frame_buffer() const;

  webrtc::VideoFrame get() const;

 private:
  webrtc::VideoFrame frame_;
};

class VideoFrameBuilder : webrtc::RefCountInterface {
 public:
  VideoFrameBuilder() = default;

  void set_video_frame_buffer(const VideoFrameBuffer& buffer);
  void set_timestamp_us(int64_t timestamp_us);
  void set_rotation(lkVideoRotation rotation);
  void set_id(uint16_t id);
  webrtc::scoped_refptr<VideoFrame> build();

 private:
  webrtc::VideoFrame::Builder builder_;
};

webrtc::scoped_refptr<VideoFrameBuilder> new_video_frame_builder();

}  // namespace livekit_ffi
