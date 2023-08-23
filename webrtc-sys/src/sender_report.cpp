#include "livekit/sender_report.h"

namespace livekit {

SenderReport::SenderReport(
    std::unique_ptr<webrtc::LTSenderReport> sender_report) 
    : sender_report_(std::move(sender_report)) {}


uint32_t SenderReport::rtp_timestamp() const {
    return sender_report_->sender_report_.rtp_timestamp();
}

int64_t SenderReport::ntp_time_ms() const {
    return sender_report_->sender_report_.ntp().ToMs();
}

}

