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
#include <vector>

#include "api/peer_connection_interface.h"
#include "api/rtp_receiver_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/media_stream.h"
#include "livekit_rtc/media_stream_track.h"
#include "livekit_rtc/rtp_parameters.h"
#include "livekit_rtc/stats.h"

namespace livekit {

class RtpReceiver : public webrtc::RefCountInterface {
 public:
  RtpReceiver(webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver,
              webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection);

  virtual ~RtpReceiver() = default;

  webrtc::scoped_refptr<MediaStreamTrack> track() const;

  void get_stats(onStatsDeliveredCallback on_stats, void* userdata) const;

  std::vector<std::string> stream_ids() const;
  std::vector<webrtc::scoped_refptr<MediaStream>> streams() const;

  lkMediaType media_type() const;
  std::string id() const;

  webrtc::scoped_refptr<RtpParameters> get_parameters() const;

  // bool set_parameters(webrtc::scoped_refptr<RtpParameters> parameters) const; // Seems unsupported

  void set_jitter_buffer_minimum_delay(bool is_some, double delay_seconds) const;

  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> rtc_receiver() const { return receiver_; }

 private:
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

}  // namespace livekit
