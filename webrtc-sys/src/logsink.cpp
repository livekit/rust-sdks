/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include <memory>

#include "livekit/logsink.h"

namespace livekit {

LogSink::LogSink(rust::Fn<void(rust::String message, LoggingSeverity severity)> fnc) : fnc_(fnc) {
  rtc::LogMessage::AddLogToStream(this, rtc::LoggingSeverity::LS_VERBOSE);
}

LogSink::~LogSink() {
  rtc::LogMessage::RemoveLogToStream(this);
}

void LogSink::OnLogMessage(const std::string& message, rtc::LoggingSeverity severity) {
  fnc_(rust::String(message), static_cast<LoggingSeverity>(severity));
}

std::unique_ptr<LogSink> new_log_sink(rust::Fn<void (rust::String, LoggingSeverity)> fnc) {
  return std::make_unique<LogSink>(fnc);
}

}

