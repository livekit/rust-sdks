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

#include "livekit_rtc/video_frame.h"

#include <memory>

#include "api/video/video_frame.h"
#include "rtc_base/logging.h"

namespace livekit_ffi {
VideoFrame::VideoFrame(const webrtc::VideoFrame& frame)
    : frame_(std::move(frame)) {}

VideoFrame::~VideoFrame() {
  //RTC_LOG(LS_INFO) << "VideoFrame destroyed";
}

unsigned int VideoFrame::width() const {
  return frame_.width();
}
unsigned int VideoFrame::height() const {
  return frame_.height();
}
uint32_t VideoFrame::size() const {
  return frame_.size();
}
uint16_t VideoFrame::id() const {
  return frame_.id();
}
int64_t VideoFrame::timestamp_us() const {
  return frame_.timestamp_us();
}
int64_t VideoFrame::ntp_time_ms() const {
  return frame_.ntp_time_ms();
}
uint32_t VideoFrame::timestamp() const {
  return frame_.rtp_timestamp();
}

lkVideoRotation VideoFrame::rotation() const {
  return static_cast<lkVideoRotation>(frame_.rotation());
}

webrtc::scoped_refptr<VideoFrameBuffer> VideoFrame::video_frame_buffer() const {
  return webrtc::make_ref_counted<VideoFrameBuffer>(frame_.video_frame_buffer());
}

webrtc::VideoFrame VideoFrame::get() const {
  return frame_;
}

void VideoFrameBuilder::set_video_frame_buffer(const VideoFrameBuffer& buffer) {
  builder_.set_video_frame_buffer(buffer.get());  // const & ref_counted
}

void VideoFrameBuilder::set_timestamp_us(int64_t timestamp_us) {
  builder_.set_timestamp_us(timestamp_us);
}

void VideoFrameBuilder::set_rotation(lkVideoRotation rotation) {
  builder_.set_rotation(static_cast<webrtc::VideoRotation>(rotation));
}

void VideoFrameBuilder::set_id(uint16_t id) {
  builder_.set_id(id);
}

webrtc::scoped_refptr<VideoFrame> VideoFrameBuilder::build() {
  return webrtc::make_ref_counted<VideoFrame>(builder_.build());
}

webrtc::scoped_refptr<VideoFrameBuilder> new_video_frame_builder() {
  return webrtc::make_ref_counted<VideoFrameBuilder>();
}

}  // namespace livekit_ffi
