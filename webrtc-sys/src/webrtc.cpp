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

#include "livekit/webrtc.h"

#include "rtc_base/logging.h"

namespace livekit {
RTCRuntime::RTCRuntime() {
  rtc::LogMessage::LogToDebug(rtc::LS_INFO);
  RTC_LOG(LS_INFO) << "RTCRuntime()";
  RTC_CHECK(rtc::InitializeSSL()) << "Failed to InitializeSSL()";

  network_thread_ = rtc::Thread::CreateWithSocketServer();
  network_thread_->SetName("network_thread", &network_thread_);
  network_thread_->Start();
  worker_thread_ = rtc::Thread::Create();
  worker_thread_->SetName("worker_thread", &worker_thread_);
  worker_thread_->Start();
  signaling_thread_ = rtc::Thread::Create();
  signaling_thread_->SetName("signaling_thread", &signaling_thread_);
  signaling_thread_->Start();
}

RTCRuntime::~RTCRuntime() {
  RTC_LOG(LS_INFO) << "~RTCRuntime()";

  rtc::ThreadManager::Instance()->SetCurrentThread(nullptr);
  RTC_CHECK(rtc::CleanupSSL()) << "Failed to CleanupSSL()";

  worker_thread_->Stop();
  signaling_thread_->Stop();
  network_thread_->Stop();
}

rtc::Thread* RTCRuntime::network_thread() const {
  return network_thread_.get();
}

rtc::Thread* RTCRuntime::worker_thread() const {
  return worker_thread_.get();
}

rtc::Thread* RTCRuntime::signaling_thread() const {
  return signaling_thread_.get();
}

std::shared_ptr<RTCRuntime> create_rtc_runtime() {
  return std::make_shared<RTCRuntime>();
}
}  // namespace livekit
