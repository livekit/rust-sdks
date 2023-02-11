//
// Created by theom on 04/09/2022.
//

#pragma once

#include "api/rtc_error.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/rtc_error.rs.h"

namespace livekit {

RTCError to_error(const webrtc::RTCError& error);
std::string serialize_error(
    const RTCError& error);  // to be used inside cxx::Exception msg

#ifdef LIVEKIT_TEST
rust::String serialize_deserialize();
void throw_error();
#endif

}  // namespace livekit
