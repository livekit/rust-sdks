//
// Created by Théo Monnom on 02/09/2022.
//

#include "livekit/rtp_transceiver.h"

namespace livekit {
RtpTransceiver::RtpTransceiver(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver)
    : transceiver_(std::move(transceiver)) {}
}  // namespace livekit