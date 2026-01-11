#ifndef LIVEKIT_SESSION_DESCRIPTION_H
#define LIVEKIT_SESSION_DESCRIPTION_H

#include <memory>

#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/scoped_refptr.h"

namespace livekit {

class SessionDescription : public webrtc::RefCountInterface {
 public:
  SessionDescription(std::unique_ptr<webrtc::SessionDescriptionInterface> desc)
      : desc_(std::move(desc)) {}

  static webrtc::scoped_refptr<SessionDescription> Create(
      std::string sdp, webrtc::SdpType type) {
    webrtc::SdpParseError error;
    std::unique_ptr<webrtc::SessionDescriptionInterface> desc =
        webrtc::CreateSessionDescription(type, sdp, &error);
    if (!desc) {
      return nullptr;
    }
    return webrtc::make_ref_counted<SessionDescription>(std::move(desc));
  }

  static webrtc::scoped_refptr<SessionDescription> Create(
      const webrtc::SessionDescriptionInterface* desc) {
    std::string sdp;
    desc->ToString(&sdp);
    return webrtc::make_ref_counted<SessionDescription>(
        webrtc::CreateSessionDescription(desc->GetType(), sdp));
  }

  const std::string ToString() const {
    std::string sdp;
    desc_->ToString(&sdp);
    return sdp;
  }

  webrtc::SdpType GetType() const { return desc_->GetType(); }

  std::unique_ptr<webrtc::SessionDescriptionInterface> Clone() const {
    std::string sdp;
    desc_->ToString(&sdp);
    return webrtc::CreateSessionDescription(desc_->GetType(), ToString());
  }

 private:
  std::unique_ptr<webrtc::SessionDescriptionInterface> desc_;
  std::string sdp_;
};

}  // namespace livekit

#endif  // LIVEKIT_SESSION_DESCRIPTION_H