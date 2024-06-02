#ifndef LIVEKIT_UTILS_H
#define LIVEKIT_UTILS_H

#include "api/peer_connection_interface.h"
#include "api/rtc_error.h"
#include "livekit/capi.h"

namespace livekit {

lkRtcError toRtcError(const webrtc::RTCError& error);

webrtc::PeerConnectionInterface::RTCOfferAnswerOptions
toNativeOfferAnswerOptions(const lkOfferAnswerOptions& options);

}  // namespace livekit

#endif  // LIVEKIT_UTILS_H
