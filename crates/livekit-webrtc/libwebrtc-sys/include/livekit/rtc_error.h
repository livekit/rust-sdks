//
// Created by theom on 04/09/2022.
//

#ifndef CLIENT_SDK_NATIVE_RTC_ERROR_H
#define CLIENT_SDK_NATIVE_RTC_ERROR_H

#include "api/rtc_error.h"
#include "libwebrtc-sys/src/rtc_error.rs.h"
#include "rust/cxx.h"
#include "rust_types.h"

namespace livekit {

RTCError to_error(const webrtc::RTCError& error);
std::string serialize_error(
    const RTCError& error);  // to be used inside cxx::Exception msg

#ifdef LIVEKIT_TEST
rust::String serialize_deserialize();
void throw_error();
#endif

}  // namespace livekit

#endif  // CLIENT_SDK_NATIVE_RTC_ERROR_H
