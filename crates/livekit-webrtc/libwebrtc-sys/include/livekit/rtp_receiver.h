//
// Created by Théo Monnom on 01/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTP_RECEIVER_H
#define CLIENT_SDK_NATIVE_RTP_RECEIVER_H

#include <memory>

#include "api/rtp_receiver_interface.h"

namespace livekit {

class RtpReceiver {
 public:
  explicit RtpReceiver(
      rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

 private:
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
};

static std::unique_ptr<RtpReceiver> _unique_rtp_receiver() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_RECEIVER_H
