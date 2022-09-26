//
// Created by Théo Monnom on 02/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H
#define CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H

#include <memory>

#include "api/rtp_transceiver_interface.h"

namespace livekit {

class RtpTransceiver {
 public:
  explicit RtpTransceiver(
      rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver);

 private:
  rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver_;
};

static std::unique_ptr<RtpTransceiver> _unique_rtp_transceiver() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H
