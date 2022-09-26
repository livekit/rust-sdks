//
// Created by theom on 18/09/2022.
//

#ifndef LIVEKIT_WEBRTC_WEBRTC_H
#define LIVEKIT_WEBRTC_WEBRTC_H

#include "rtc_base/physical_socket_server.h"
#include "rtc_base/ssl_adapter.h"

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
#ifdef WEBRTC_WIN
  rtc::WinsockInitializer winsock_;
#endif
};

std::unique_ptr<RTCRuntime> create_rtc_runtime();

}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_WEBRTC_H
