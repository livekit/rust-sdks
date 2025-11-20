#ifndef LIVEKIT_RTC_ICE_CANDIDATE_H_
#define LIVEKIT_RTC_ICE_CANDIDATE_H_

#include <memory>

#include "api/jsep.h"
#include "api/make_ref_counted.h"
#include "api/scoped_refptr.h"

namespace livekit {

class IceCandidate : public webrtc::RefCountInterface {
 public:
  IceCandidate(std::unique_ptr<webrtc::IceCandidateInterface> candidate)
      : candidate_(std::move(candidate)) {}

  webrtc::IceCandidateInterface* GetCandidate() const {
    return candidate_.get();
  }

  std::string mid() const { return candidate_->sdp_mid(); }

  int mline_index() const { return candidate_->sdp_mline_index(); }

  std::string sdp() const {
    std::string sdp;
    candidate_->ToString(&sdp);
    return sdp;
  }

  static webrtc::scoped_refptr<IceCandidate> Create(const std::string& sdp_mid,
                                                    int sdp_mline_index,
                                                    const std::string& sdp) {
    webrtc::SdpParseError error;
    std::unique_ptr<webrtc::IceCandidateInterface> candidate(
        webrtc::CreateIceCandidate(sdp_mid, sdp_mline_index, sdp, &error));
    if (!candidate) {
      return nullptr;
    }
    return webrtc::make_ref_counted<IceCandidate>(std::move(candidate));
  }

  std::unique_ptr<webrtc::IceCandidateInterface> Clone() const {
    std::string sdp;
    candidate_->ToString(&sdp);
    return std::unique_ptr<webrtc::IceCandidateInterface>(
        webrtc::CreateIceCandidate(candidate_->sdp_mid(),
                                   candidate_->sdp_mline_index(),
                                   sdp,
                                   nullptr));
  }

 private:
  std::unique_ptr<webrtc::IceCandidateInterface> candidate_;
};

}  // namespace livekit

#endif  // LIVEKIT_RTC_ICE_CANDIDATE_H_