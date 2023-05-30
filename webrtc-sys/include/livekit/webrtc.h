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

#pragma once

#include "rtc_base/physical_socket_server.h"
#include "rtc_base/ssl_adapter.h"
#include "rust/cxx.h"

#ifdef WEBRTC_WIN
#include "rtc_base/win32_socket_init.h"
#endif

namespace livekit {
class RtcRuntime;
}
#include "webrtc-sys/src/webrtc.rs.h"

namespace livekit {

class RtcRuntime {
 public:
  RtcRuntime();
  ~RtcRuntime();

  RtcRuntime(const RtcRuntime&) = delete;
  RtcRuntime& operator=(const RtcRuntime&) = delete;

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

rust::String create_random_uuid();

}  // namespace livekit
