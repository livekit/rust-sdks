//
// Created by theom on 18/09/2022.
//

#ifndef LIVEKIT_WEBRTC_WEBRTC_H
#define LIVEKIT_WEBRTC_WEBRTC_H

#include "rtc_base/ssl_adapter.h"
#include "rtc_base/physical_socket_server.h"

#ifdef WEBRTC_WIN
#include "rtc_base/win32_socket_init.h"
#endif

namespace livekit {

    class RTCRuntime {
    public:
        RTCRuntime();
        ~RTCRuntime();

        RTCRuntime(const RTCRuntime&) = delete;
        RTCRuntime& operator=(const RTCRuntime&) = delete;
    private:
        rtc::WinsockInitializer winsock_;
    };

    std::unique_ptr<RTCRuntime> create_rtc_runtime();

} // livekit

#endif //LIVEKIT_WEBRTC_WEBRTC_H
