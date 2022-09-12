//
// Created by theom on 04/09/2022.
//

#include "livekit/rtc_error.h"

namespace livekit {
    RTCError::RTCError(webrtc::RTCError error) : rtc_error_(std::move(error)) {

    }
} // livekit