#ifndef LIVEKIT_UTILS_H
#define LIVEKIT_UTILS_H

#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "livekit_rtc/capi.h"

namespace livekit {

lkRtcError toRtcError(const webrtc::RTCError& error);

webrtc::PeerConnectionInterface::RTCOfferAnswerOptions
    toNativeOfferAnswerOptions(const lkOfferAnswerOptions& options);

class String : public webrtc::RefCountInterface {
 public:
  explicit String(const std::string& str) : str_(str) {}

  std::string get() const { return str_; }

 private:
  std::string str_;
};

template <typename T>
class Vec : public webrtc::RefCountInterface {
 public:
  explicit Vec();

  explicit Vec(const std::vector<T>& vec) : vec_(vec) {}

  std::vector<T> get() const { return vec_; }

 private:
  std::vector<T> vec_;
};

}  // namespace livekit

#endif  // LIVEKIT_UTILS_H
