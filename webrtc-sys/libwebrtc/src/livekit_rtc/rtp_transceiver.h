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
#include "api/rtp_parameters.h"
#include "api/rtp_transceiver_direction.h"
#include "api/rtp_transceiver_interface.h"
#include "api/scoped_refptr.h"
#include "livekit_rtc/rtc_error.h"
#include "livekit_rtc/rtp_receiver.h"
#include "livekit_rtc/rtp_sender.h"

namespace livekit {
class RtpTransceiver;
}

namespace livekit {

webrtc::RtpTransceiverInit to_native_rtp_transceiver_init(RtpTransceiverInit init);

class RtpTransceiver : public webrtc::RefCountInterface {
 public:
  RtpTransceiver(webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver,
                 webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection);

  virtual ~RtpTransceiver() = default;

  lkMediaType media_type() const;

  std::string mid() const;

  webrtc::scoped_refptr<RtpSender> sender() const;

  webrtc::scoped_refptr<RtpReceiver> receiver() const;

  bool stopped() const;

  bool stopping() const;

  lkRtpTransceiverDirection direction() const;

  void set_direction(lkRtpTransceiverDirection direction) const;

  lkRtpTransceiverDirection current_direction() const;

  lkRtpTransceiverDirection fired_direction() const;

  void stop_standard() const;

  void set_codec_preferences(std::vector<RtpCodecCapability> codecs) const;

  std::vector<RtpCodecCapability> codec_preferences() const;

  std::vector<RtpHeaderExtensionCapability> header_extensions_to_negotiate() const;

  std::vector<RtpHeaderExtensionCapability> negotiated_header_extensions() const;

  void set_header_extensions_to_negotiate(
      std::vector<RtpHeaderExtensionCapability> header_extensions_to_offer) const;
  
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection() const {
    return peer_connection_;
  }
 private:
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver_;
  webrtc::scoped_refptr<webrtc::PeerConnectionInterface> peer_connection_;
};

}  // namespace livekit
