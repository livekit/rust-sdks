#pragma once

#include "livekit/frame_transformer.h"
#include "rtc_base/checks.h"

namespace livekit {
class SenderReport;
}  // namespace livekit

#include "webrtc-sys/src/sender_report.rs.h"


namespace livekit {

class SenderReport {
 public:
  explicit SenderReport(std::unique_ptr<webrtc::LTSenderReport> sender_report);

 private:
  std::unique_ptr<webrtc::LTSenderReport> sender_report_;
};

}  // namespace livekit