//
// Created by Théo Monnom on 01/09/2022.
//

#include "livekit/rtp_receiver.h"

namespace livekit {
RtpReceiver::RtpReceiver(
    rtc::scoped_refptr<webrtc::RtpReceiverInterface> receiver)
    : receiver_(std::move(receiver)) {}

std::unique_ptr<MediaStreamTrack> RtpReceiver::track() const {
  return std::make_unique<MediaStreamTrack>(receiver_->track());
}

}  // namespace livekit