/*
 * Copyright 2026 LiveKit, Inc.
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

// Minimal stand-in for WebRTC's rtc_base/logging.h, so that standalone tests
// can compile native sources without libwebrtc. Only RTC_LOG(severity) << ...
// is supported; verbose and info messages are dropped. Test include paths
// only; never use in production builds.

#ifndef WEBRTC_SYS_TEST_SHIMS_RTC_BASE_LOGGING_H_
#define WEBRTC_SYS_TEST_SHIMS_RTC_BASE_LOGGING_H_

#include <iostream>
#include <sstream>

namespace test_shim {

enum LoggingSeverity { LS_VERBOSE, LS_INFO, LS_WARNING, LS_ERROR };

class LogLine {
 public:
  explicit LogLine(LoggingSeverity severity) : severity_(severity) {}
  ~LogLine() {
    if (severity_ >= LS_WARNING) {
      std::cerr << "[log] " << stream_.str() << std::endl;
    }
  }
  std::ostream& stream() { return stream_; }

 private:
  LoggingSeverity severity_;
  std::ostringstream stream_;
};

}  // namespace test_shim

#define RTC_LOG(severity) ::test_shim::LogLine(::test_shim::severity).stream()

#endif  // WEBRTC_SYS_TEST_SHIMS_RTC_BASE_LOGGING_H_
