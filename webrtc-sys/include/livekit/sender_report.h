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

  uint32_t ssrc() const;
  uint32_t rtp_timestamp() const;
  int64_t ntp_time_ms() const;

 private:
  std::unique_ptr<webrtc::LTSenderReport> sender_report_;
};

}  // namespace livekit