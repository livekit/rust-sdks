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

#include <exception>
#include <string>
#include <utility>

#include "api/rtc_error.h"
#include "rust/cxx.h"
#include "webrtc-sys/src/rtc_error.rs.h"

namespace livekit_ffi {

RtcError to_error(const webrtc::RTCError& error);
std::string serialize_error(
    const RtcError& error);  // to be used inside cxx::Exception msg

// The exception type to throw when an `extern "C++"` function declared
// `-> Result<...>` has to report an error to Rust.
//
// Never throw `std::runtime_error` (or any other standard exception) with a
// temporary string from this crate. The prebuilt libwebrtc is built with
// `_HAS_EXCEPTIONS=0`, and `webrtc_sys_build::webrtc_defines()` copies every
// `-D` out of `webrtc.ninja` onto our own translation units, so that macro
// applies here too. Under it the MSVC STL's `runtime_error(const string&)`
// stores the pointer instead of copying the message
// (https://github.com/microsoft/STL/issues/2114), so `what()` dangles the
// moment the temporary is destroyed at the end of the throw expression. cxx
// then calls `strlen` on freed memory and hands the result to
// `cxxbridge1$exception`, which trips a `ptr::copy_nonoverlapping`
// precondition check in debug builds and aborts the process. Owning the
// message keeps `what()` valid regardless of how the STL is configured.
class BridgeException final : public std::exception {
 public:
  explicit BridgeException(std::string message)
      : message_(std::move(message)) {}

  const char* what() const noexcept override { return message_.c_str(); }

 private:
  std::string message_;
};

#ifdef LIVEKIT_TEST
rust::String serialize_deserialize();
#endif

}  // namespace livekit_ffi
