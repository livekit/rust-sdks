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

#include <memory>

#include "api/peer_connection_interface.h"
#include "api/rtp_sender_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/media_stream.h"
#include "livekit_rtc/rtc_error.h"
#include "livekit_rtc/rtp_parameters.h"
#include "livekit_rtc/stats.h"

namespace livekit {

class PeerFactory;
class MediaStreamTrack;

class RtpSender : public webrtc::RefCountInterface {
 public:
  RtpSender(webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender,
            webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection);

  bool set_track(webrtc::scoped_refptr<MediaStreamTrack> track) const;

  webrtc::scoped_refptr<MediaStreamTrack> track() const;

  uint32_t ssrc() const;

  void get_stats(onStatsDeliveredCallback on_stats, void* userdata) const;

  lkMediaType media_type() const;

  std::string id() const;

  std::vector<std::string> stream_ids() const;

  void set_streams(const std::vector<std::string>& stream_ids) const;

  std::vector<RtpEncodingParameters> init_send_encodings() const;

  RtpParameters get_parameters() const;

  void set_parameters(RtpParameters params) const;

  webrtc::scoped_refptr<webrtc::RtpSenderInterface> rtc_sender() const { return sender_; }

 private:
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

}  // namespace livekit
