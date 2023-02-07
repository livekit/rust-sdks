//
// Created by Théo Monnom on 02/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H
#define CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H

#include <memory>

#include "api/rtp_parameters.h"
#include "api/rtp_transceiver_direction.h"
#include "api/rtp_transceiver_interface.h"
#include "livekit/rtc_error.h"
#include "livekit/rtp_receiver.h"
#include "livekit/rtp_sender.h"
#include "rust/cxx.h"
#include "rust_types.h"
#include "webrtc-sys/src/rtc_error.rs.h"
#include "webrtc-sys/src/rtp_parameters.rs.h"

namespace livekit {

class RtpTransceiver {
 public:
  explicit RtpTransceiver(
      rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver);

  MediaType media_type() const {
    return static_cast<MediaType>(transceiver_->media_type());
  }

  rust::String mid() const {
    // The Result is converted into an Option in Rust (Wait for Option suport in
    // cxx.rs)
    return transceiver_->mid().value();
  }

  std::shared_ptr<RtpSender> sender() const {
    return std::make_shared<RtpSender>(transceiver_->sender());
  }

  std::shared_ptr<RtpReceiver> receiver() const {
    return std::make_shared<RtpReceiver>(transceiver_->receiver());
  }

  bool stopped() const { return transceiver_->stopped(); }

  bool stopping() const { return transceiver_->stopping(); }

  RtpTransceiverDirection direction() const {
    return static_cast<RtpTransceiverDirection>(transceiver_->direction());
  }

  void set_direction(RtpTransceiverDirection direction) const {
    auto error = transceiver_->SetDirectionWithError(
        static_cast<webrtc::RtpTransceiverDirection>(direction));

    if (!error.ok()) {
      throw std::runtime_error(serialize_error(to_error(error)));
    }
  }

  RtpTransceiverDirection current_direction() const {
    return static_cast<RtpTransceiverDirection>(
        transceiver_->current_direction().value());
  }

  RtpTransceiverDirection fired_direection() const {
    return static_cast<RtpTransceiverDirection>(
        transceiver_->fired_direction().value());
  }

  void stop_standard() const {
    auto error = transceiver_->StopStandard();
    if (!error.ok())
      throw std::runtime_error(serialize_error(to_error(error)));
  }

  void set_codec_preferences(rust::Vec<RtpCodecCapability> codecs) const {
    // TODO Convert RtpCodecCapability
  }

  // TODO Other functions

 private:
  rtc::scoped_refptr<webrtc::RtpTransceiverInterface> transceiver_;
};

static std::shared_ptr<RtpTransceiver> _shared_rtp_transceiver() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_TRANSCEIVER_H
