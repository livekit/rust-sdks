#ifndef CLIENT_SDK_NATIVE_RTP_SENDER_H
#define CLIENT_SDK_NATIVE_RTP_SENDER_H

#include <memory>

#include "api/rtp_sender_interface.h"
#include "livekit/media_stream.h"

namespace livekit {

class RtpSender {
 public:
  explicit RtpSender(rtc::scoped_refptr<webrtc::RtpSenderInterface> sender);

  bool set_track(MediaStreamTrack* track) {}
  std::unique_ptr<MediaStreamTrack> track() const {
    return MediaStreamTrack::from(sender_->track());
  }

  uint32_t ssrc() const { return sender_->ssrc(); }
  rust::String id() const { return sender_->id(); }
  rust::Vec<rust::String> stream_ids() const;

 private:
  rtc::scoped_refptr<webrtc::RtpSenderInterface> sender_;
};

static std::unique_ptr<RtpSender> _unique_rtp_sender() {
  return nullptr;  // Ignore
}
}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTP_SENDER_H
