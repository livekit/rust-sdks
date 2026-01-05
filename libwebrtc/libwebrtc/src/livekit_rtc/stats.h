#pragma once

#include "api/stats/rtc_stats_collector_callback.h"

namespace livekit {

using onStatsDeliveredCallback = void (*)(const char* statsJson, void* userdata);

class NativeRtcStatsCollector : public webrtc::RTCStatsCollectorCallback {
 public:
  NativeRtcStatsCollector(onStatsDeliveredCallback on_stats, void* userdata)
      : userdata_(userdata), on_stats_(on_stats) {}

  void OnStatsDelivered(
      const webrtc::scoped_refptr<const webrtc::RTCStatsReport>& report) override {
    stats_json_ = report->ToJson();
    on_stats_(stats_json_.c_str(), userdata_);
  }

 private:
  void* userdata_;
  std::string stats_json_;
  onStatsDeliveredCallback on_stats_;
};

}  // namespace livekit
