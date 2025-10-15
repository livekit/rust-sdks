/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include "api/rtc_error.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/rtc_error.rs.h"

namespace livekit {

RtcError to_error(const webrtc::RTCError& error);
std::string serialize_error(
    const RtcError& error);  // to be used inside cxx::Exception msg

#ifdef LIVEKIT_TEST
rust::String serialize_deserialize();
void throw_error();
#endif

}  // namespace livekit
