/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/rtp_receiver.h"
#include "livekit/frame_transformer.h"

#include "absl/types/optional.h"

namespace livekit {

RtpReceiver::RtpReceiver(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : receiver_(std::move(receiver)) {}

std::shared_ptr<MediaStreamTrack> RtpReceiver::track() const {
  return MediaStreamTrack::from(receiver_->track());
}

rust::Vec<rust::String> RtpReceiver::stream_ids() const {
  rust::Vec<rust::String> rust;
  for (auto id : receiver_->stream_ids())
    rust.push_back(id);
  return rust;
}

rust::Vec<MediaStreamPtr> RtpReceiver::streams() const {
  rust::Vec<MediaStreamPtr> rust;
  for (auto stream : receiver_->streams())
    rust.push_back(MediaStreamPtr{std::make_shared<MediaStream>(stream)});
  return rust;
}

MediaType RtpReceiver::media_type() const {
  return static_cast<MediaType>(receiver_->media_type());
}

rust::String RtpReceiver::id() const {
  return receiver_->id();
}

RtpParameters RtpReceiver::get_parameters() const {
  return to_rust_rtp_parameters(receiver_->GetParameters());
}

void RtpReceiver::set_depacketizer_to_decoder_frame_transformer(std::shared_ptr<livekit::FrameTransformerInterface> frame_transformer) const {
  fprintf(stderr, "RtpReceiver::set_depacketizer_to_decoder_frame_transformer");
}

void RtpReceiver::set_jitter_buffer_minimum_delay(bool is_some,
                                                  double delay_seconds) const {
  receiver_->SetJitterBufferMinimumDelay(
      is_some ? absl::make_optional(delay_seconds) : absl::nullopt);
}

}  // namespace livekit
