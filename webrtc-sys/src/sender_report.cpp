#include "livekit/sender_report.h"

namespace livekit {

SenderReport::SenderReport(
    std::unique_ptr<webrtc::LTSenderReport> sender_report) 
    : sender_report_(std::move(sender_report)) {

}

}