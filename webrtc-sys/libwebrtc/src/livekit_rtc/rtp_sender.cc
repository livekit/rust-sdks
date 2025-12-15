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

#include "livekit_rtc/rtp_sender.h"

#include "livekit_rtc/media_stream_track.h"
#include "livekit_rtc/rtp_parameters.h"

namespace livekit {

RtpSender::RtpSender(
    webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender,
    webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection)
    : sender_(std::move(sender)),
      peer_connection_(std::move(peer_connection)) {}

bool RtpSender::set_track(webrtc::scoped_refptr<MediaStreamTrack> track) const {
  return sender_->SetTrack(track->track());
}

webrtc::scoped_refptr<MediaStreamTrack> RtpSender::track() const {
  return webrtc::make_ref_counted<MediaStreamTrack>(sender_->track());
  // return pc_factory_->get_or_create_media_stream_track(sender_->track());
}

uint32_t RtpSender::ssrc() const {
  return sender_->ssrc();
}

void RtpSender::get_stats(onStatsDeliveredCallback on_stats,
                          void* userdata) const {
  auto observer =
      webrtc::make_ref_counted<NativeRtcStatsCollector>(on_stats, userdata);
  peer_connection_->GetStats(sender_, observer);
}

lkMediaType RtpSender::media_type() const {
  return static_cast<lkMediaType>(sender_->media_type());
}

std::string RtpSender::id() const {
  return sender_->id();
}

std::vector<std::string> RtpSender::stream_ids() const {
  return sender_->stream_ids();
}

void RtpSender::set_streams(const std::vector<std::string>& stream_ids) const {
  sender_->SetStreams(stream_ids);
}

std::vector<webrtc::scoped_refptr<RtpEncodingParameters>>
RtpSender::init_send_encodings() const {
  std::vector<webrtc::scoped_refptr<RtpEncodingParameters>> encodings;
  for (auto encoding : sender_->init_send_encodings())
    encodings.push_back(RtpEncodingParameters::FromNative(encoding));
  return encodings;
}

webrtc::scoped_refptr<RtpParameters> RtpSender::get_parameters() const {
  return RtpParameters::FromNative(sender_->GetParameters());
}

void RtpSender::set_parameters(
    webrtc::scoped_refptr<RtpParameters> params) const {
  auto error = sender_->SetParameters(params->rtc_parameters());
  if (!error.ok()) {
    // throw std::runtime_error(serialize_error(to_error(error)));
  }
}

}  // namespace livekit
