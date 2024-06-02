#ifndef LIVEKIT_TRANSCEIVER_H
#define LIVEKIT_TRANSCEIVER_H

#include "api/rtp_transceiver_interface.h"
#include "api/scoped_refptr.h"
#include "rtc_base/ref_count.h"

namespace livekit {

class RtpSender : public rtc::RefCountInterface {
 public:
  RtpSender(rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

 private:
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
};

class RtpReceiver : public rtc::RefCountInterface {
 public:
  RtpReceiver(rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

 private:
  rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
};

class RtpTransceiver : public rtc::RefCountInterface {
 public:
  RtpTransceiver(
      rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver);

 private:
  rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver_;
  rtc::scoped_refptr<RtpSender> sender_ = nullptr;
  rtc::scoped_refptr<RtpReceiver> receiver_ = nullptr;
};
}  // namespace livekit

#endif  // LIVEKIT_TRANSCEIVER_H
