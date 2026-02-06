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

#include "livekit_rtc/rtp_receiver.h"

#include <memory>

#include "absl/types/optional.h"
#include "api/peer_connection_interface.h"
#include "api/scoped_refptr.h"

namespace livekit_ffi {

RtpReceiver::RtpReceiver(
    webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : receiver_(std::move(receiver)),
      peer_connection_(std::move(peer_connection)) {}

webrtc::scoped_refptr<MediaStreamTrack> RtpReceiver::track() const {
  return webrtc::make_ref_counted<MediaStreamTrack>(receiver_->track());
  // TODO: return
  // rtc_runtime_->get_or_create_media_stream_track(receiver_->track());
}

std::vector<std::string> RtpReceiver::stream_ids() const {
  return receiver_->stream_ids();
}

void RtpReceiver::get_stats(onStatsDeliveredCallback on_stats,
                            void* userdata) const {
  auto observer =
      webrtc::make_ref_counted<NativeRtcStatsCollector>(on_stats, userdata);
  peer_connection_->GetStats(receiver_, observer);
}

std::vector<webrtc::scoped_refptr<MediaStream>> RtpReceiver::streams() const {
  std::vector<webrtc::scoped_refptr<MediaStream>> vec;
  for (auto stream : receiver_->streams())
    vec.push_back(webrtc::make_ref_counted<MediaStream>(stream));
  return vec;
}

lkMediaType RtpReceiver::media_type() const {
  return static_cast<lkMediaType>(receiver_->media_type());
}

std::string RtpReceiver::id() const {
  return receiver_->id();
}

webrtc::scoped_refptr<RtpParameters> RtpReceiver::get_parameters() const {
  return RtpParameters::FromNative(receiver_->GetParameters());
}

void RtpReceiver::set_jitter_buffer_minimum_delay(bool is_some,
                                                  double delay_seconds) const {
  receiver_->SetJitterBufferMinimumDelay(
      is_some ? absl::make_optional(delay_seconds) : absl::nullopt);
}

}  // namespace livekit_ffi
