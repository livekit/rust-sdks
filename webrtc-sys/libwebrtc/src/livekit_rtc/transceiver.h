#ifndef LIVEKIT_TRANSCEIVER_H
#define LIVEKIT_TRANSCEIVER_H

#include "api/rtp_transceiver_interface.h"
#include "api/scoped_refptr.h"
#include "rtc_base/ref_count.h"

namespace livekit {

class RtpSender : public webrtc::RefCountInterface {
 public:
  RtpSender(webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

 private:
  webrtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
};

class RtpReceiver : public webrtc::RefCountInterface {
 public:
  RtpReceiver(webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver);

 private:
  webrtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver_;
};

class RtpTransceiver : public webrtc::RefCountInterface {
 public:
  RtpTransceiver(
      webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver);

 private:
  webrtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver_;
  webrtc::scoped_refptr<RtpSender> sender_ = nullptr;
  webrtc::scoped_refptr<RtpReceiver> receiver_ = nullptr;
};
}  // namespace livekit

#endif  // LIVEKIT_TRANSCEIVER_H
