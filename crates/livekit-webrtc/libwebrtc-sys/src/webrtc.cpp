//
// Created by theom on 18/09/2022.
//

#include "livekit/webrtc.h"

#include "rtc_base/logging.h"

namespace livekit {
RTCRuntime::RTCRuntime() {
  // rtc::LogMessage::LogToDebug(rtc::LS_INFO);
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
  RTC_CHECK(rtc::CleanupSSL()) << "Failed to CleanupSSL()";
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