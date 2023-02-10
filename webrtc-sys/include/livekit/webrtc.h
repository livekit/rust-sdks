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

  rtc::Thread* network_thread() const;
  rtc::Thread* worker_thread() const;
  rtc::Thread* signaling_thread() const;

 private:
  std::unique_ptr<rtc::Thread> network_thread_;
  std::unique_ptr<rtc::Thread> worker_thread_;
  std::unique_ptr<rtc::Thread> signaling_thread_;

#ifdef WEBRTC_WIN
  rtc::WinsockInitializer winsock_;
  rtc::PhysicalSocketServer ss_;
  rtc::AutoSocketServerThread main_thread_{&ss_};
#endif
};

std::shared_ptr<RTCRuntime> create_rtc_runtime();

}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_WEBRTC_H
