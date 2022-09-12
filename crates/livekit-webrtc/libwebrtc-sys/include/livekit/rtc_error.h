//
// Created by theom on 04/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTC_ERROR_H
#define CLIENT_SDK_NATIVE_RTC_ERROR_H

#include "api/rtc_error.h"

namespace livekit {

    class RTCError {
    public:
        explicit RTCError(webrtc::RTCError error);

    private:
        webrtc::RTCError rtc_error_;
    };

    static std::unique_ptr<RTCError> _unique_rtc_error(){
        return nullptr; // Ignore
    }
} // livekit

#endif //CLIENT_SDK_NATIVE_RTC_ERROR_H
