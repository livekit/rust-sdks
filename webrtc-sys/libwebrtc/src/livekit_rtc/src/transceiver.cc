#include "livekit/transceiver.h"

namespace livekit {

RtpSender::RtpSender(rtc::scoped_refptr<webrtc::RtpSenderInterface> sender)
    : sender_(sender) {}

RtpReceiver::RtpReceiver(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : receiver_(receiver) {}

RtpTransceiver::RtpTransceiver(
    rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver)
    : transceiver_(transceiver) {
  if (transceiver->sender()) {
    sender_ = rtc::make_ref_counted<RtpSender>(transceiver->sender());
  }

  if (transceiver->receiver()) {
    receiver_ = rtc::make_ref_counted<RtpReceiver>(transceiver->receiver());
  }
}

}  // namespace livekit
