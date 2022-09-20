//
// Created by theom on 18/09/2022.
//

#include "livekit/webrtc.h"

#include "rtc_base/logging.h"

namespace livekit {
RTCRuntime::RTCRuntime() {
  RTC_LOG(LS_INFO) << "RTCRuntime()";
  RTC_CHECK(rtc::InitializeSSL()) << "Failed to InitializeSSL()";
}

RTCRuntime::~RTCRuntime() {
  RTC_LOG(LS_INFO) << "~RTCRuntime()";
  RTC_CHECK(rtc::CleanupSSL()) << "Failed to CleanupSSL()";
}

std::unique_ptr<RTCRuntime> create_rtc_runtime() {
  return std::make_unique<RTCRuntime>();
}
}  // namespace livekit