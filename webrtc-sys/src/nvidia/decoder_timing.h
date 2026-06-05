#ifndef WEBRTC_NVIDIA_DECODER_TIMING_H_
#define WEBRTC_NVIDIA_DECODER_TIMING_H_

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iomanip>
#include <sstream>
#include <string>

#include "NvDecoder/NvDecoder.h"
#include "rtc_base/logging.h"

namespace webrtc {
namespace nvidia {

inline int64_t TimingNowUs() {
  const auto now = std::chrono::steady_clock::now().time_since_epoch();
  return std::chrono::duration_cast<std::chrono::microseconds>(now).count();
}

inline bool DecoderTimingEnabled() {
  static const bool enabled = [] {
    const char* value = std::getenv("LIVEKIT_NVIDIA_DECODER_TIMING");
    return value != nullptr && std::strcmp(value, "0") != 0 &&
           std::strcmp(value, "false") != 0 &&
           std::strcmp(value, "FALSE") != 0;
  }();
  return enabled;
}

struct DurationWindow {
  uint64_t count = 0;
  int64_t sum_us = 0;
  int64_t min_us = 0;
  int64_t max_us = 0;

  void Record(int64_t duration_us) {
    duration_us = std::max<int64_t>(duration_us, 0);
    if (count == 0) {
      min_us = duration_us;
      max_us = duration_us;
    } else {
      min_us = std::min(min_us, duration_us);
      max_us = std::max(max_us, duration_us);
    }
    count++;
    sum_us += duration_us;
  }
};

inline void RecordAggregate(DurationWindow* window,
                            int64_t total_us,
                            uint64_t count) {
  if (count == 0) {
    return;
  }
  window->Record(total_us / static_cast<int64_t>(count));
}

inline std::string FormatDuration(const DurationWindow& window) {
  if (window.count == 0) {
    return "NA";
  }
  std::ostringstream out;
  out << std::fixed << std::setprecision(1)
      << (static_cast<double>(window.sum_us) /
          static_cast<double>(window.count) / 1000.0)
      << "ms/" << (static_cast<double>(window.min_us) / 1000.0) << "ms/"
      << (static_cast<double>(window.max_us) / 1000.0) << "ms";
  return out.str();
}

class ScopedDuration {
 public:
  explicit ScopedDuration(DurationWindow* window)
      : window_(DecoderTimingEnabled() ? window : nullptr),
        start_us_(window_ == nullptr ? 0 : TimingNowUs()) {}

  ScopedDuration(const ScopedDuration&) = delete;
  ScopedDuration& operator=(const ScopedDuration&) = delete;

  ~ScopedDuration() {
    if (window_ != nullptr) {
      window_->Record(TimingNowUs() - start_us_);
    }
  }

 private:
  DurationWindow* window_;
  int64_t start_us_;
};

struct DecoderTimingWindow {
  uint64_t input_packets = 0;
  uint64_t input_bytes = 0;
  uint64_t output_frames = 0;
  uint64_t native_frames = 0;
  uint64_t cpu_fallback_frames = 0;
  uint64_t decode_calls = 0;
  uint64_t zero_return_decode_calls = 0;
  uint64_t retry_decode_calls = 0;
  uint64_t nv_picture_decode_calls = 0;
  uint64_t nv_display_calls = 0;
  uint64_t nv_frame_allocations = 0;
  DurationWindow total;
  DurationWindow bitstream_parse;
  DurationWindow nv_decode;
  DurationWindow get_frame;
  DurationWindow native_wrap;
  DurationWindow cpu_fallback_copy;
  DurationWindow callback;
  DurationWindow nv_picture_decode;
  DurationWindow nv_display;
  DurationWindow nv_map;
  DurationWindow nv_status;
  DurationWindow nv_alloc;
  DurationWindow nv_copy_submit;
  DurationWindow nv_sync;
  DurationWindow nv_unmap;
};

class DecoderTimingLogger {
 public:
  explicit DecoderTimingLogger(const char* codec_name)
      : codec_name_(codec_name), last_log_us_(TimingNowUs()) {}

  bool enabled() const { return DecoderTimingEnabled(); }

  DurationWindow* total() { return &window_.total; }
  DurationWindow* bitstream_parse() { return &window_.bitstream_parse; }
  DurationWindow* nv_decode() { return &window_.nv_decode; }
  DurationWindow* get_frame() { return &window_.get_frame; }
  DurationWindow* native_wrap() { return &window_.native_wrap; }
  DurationWindow* cpu_fallback_copy() { return &window_.cpu_fallback_copy; }
  DurationWindow* callback() { return &window_.callback; }

  void RecordInput(size_t bytes) {
    if (!enabled()) {
      return;
    }
    window_.input_packets++;
    window_.input_bytes += bytes;
  }

  void RecordDecodeCall(int returned_frames) {
    if (!enabled()) {
      return;
    }
    window_.decode_calls++;
    if (returned_frames == 0) {
      window_.zero_return_decode_calls++;
    }
  }

  void RecordDecodeRetries(int retries) {
    if (!enabled() || retries <= 0) {
      return;
    }
    window_.retry_decode_calls += static_cast<uint64_t>(retries);
  }

  void RecordOutputFrames(int frames) {
    if (!enabled() || frames <= 0) {
      return;
    }
    window_.output_frames += static_cast<uint64_t>(frames);
  }

  void RecordNativeFrame() {
    if (enabled()) {
      window_.native_frames++;
    }
  }

  void RecordCpuFallbackFrame() {
    if (enabled()) {
      window_.cpu_fallback_frames++;
    }
  }

  void RecordNvDecoderStats(const ::NvDecoder::TimingStats& stats) {
    if (!enabled()) {
      return;
    }
    window_.nv_picture_decode_calls += stats.picture_decode_calls;
    window_.nv_display_calls += stats.picture_display_calls;
    window_.nv_frame_allocations += stats.frame_allocations;
    RecordAggregate(&window_.nv_picture_decode, stats.picture_decode_us,
                    stats.picture_decode_calls);
    RecordAggregate(&window_.nv_display, stats.display_total_us,
                    stats.picture_display_calls);
    RecordAggregate(&window_.nv_map, stats.map_us, stats.picture_display_calls);
    RecordAggregate(&window_.nv_status, stats.status_us,
                    stats.picture_display_calls);
    RecordAggregate(&window_.nv_alloc, stats.alloc_us,
                    stats.picture_display_calls);
    RecordAggregate(&window_.nv_copy_submit, stats.copy_submit_us,
                    stats.picture_display_calls);
    RecordAggregate(&window_.nv_sync, stats.sync_us,
                    stats.picture_display_calls);
    RecordAggregate(&window_.nv_unmap, stats.unmap_us,
                    stats.picture_display_calls);
  }

  void MaybeLog() {
    if (!enabled()) {
      return;
    }
    const int64_t now_us = TimingNowUs();
    if (now_us - last_log_us_ < 2'000'000) {
      return;
    }
    if (window_.input_packets == 0 && window_.output_frames == 0) {
      last_log_us_ = now_us;
      return;
    }

    const double avg_input_bytes =
        window_.input_packets == 0
            ? 0.0
            : static_cast<double>(window_.input_bytes) /
                  static_cast<double>(window_.input_packets);

    RTC_LOG(LS_INFO) << "NVIDIA " << codec_name_
                     << " decoder timing: packets=" << window_.input_packets
                     << ", frames=" << window_.output_frames
                     << ", native_frames=" << window_.native_frames
                     << ", cpu_fallback_frames="
                     << window_.cpu_fallback_frames
                     << ", decode_calls=" << window_.decode_calls
                     << ", zero_return_decode_calls="
                     << window_.zero_return_decode_calls
                     << ", retry_decode_calls="
                     << window_.retry_decode_calls
                     << ", avg_input_bytes=" << avg_input_bytes
                     << ", total avg/min/max=" << FormatDuration(window_.total)
                     << ", bitstream_parse avg/min/max="
                     << FormatDuration(window_.bitstream_parse)
                     << ", nv_decode avg/min/max="
                     << FormatDuration(window_.nv_decode)
                     << ", get_frame avg/min/max="
                     << FormatDuration(window_.get_frame)
                     << ", native_wrap avg/min/max="
                     << FormatDuration(window_.native_wrap)
                     << ", cpu_fallback_copy avg/min/max="
                     << FormatDuration(window_.cpu_fallback_copy)
                     << ", callback avg/min/max="
                     << FormatDuration(window_.callback)
                     << ", nv_picture_decode_calls="
                     << window_.nv_picture_decode_calls
                     << ", nv_display_calls=" << window_.nv_display_calls
                     << ", nv_frame_allocations="
                     << window_.nv_frame_allocations
                     << ", nv_picture_decode avg/min/max="
                     << FormatDuration(window_.nv_picture_decode)
                     << ", nv_display avg/min/max="
                     << FormatDuration(window_.nv_display)
                     << ", nv_map avg/min/max="
                     << FormatDuration(window_.nv_map)
                     << ", nv_status avg/min/max="
                     << FormatDuration(window_.nv_status)
                     << ", nv_alloc avg/min/max="
                     << FormatDuration(window_.nv_alloc)
                     << ", nv_copy_submit avg/min/max="
                     << FormatDuration(window_.nv_copy_submit)
                     << ", nv_sync avg/min/max="
                     << FormatDuration(window_.nv_sync)
                     << ", nv_unmap avg/min/max="
                     << FormatDuration(window_.nv_unmap);

    window_ = DecoderTimingWindow();
    last_log_us_ = now_us;
  }

 private:
  const char* codec_name_;
  DecoderTimingWindow window_;
  int64_t last_log_us_;
};

}  // namespace nvidia
}  // namespace webrtc

#endif  // WEBRTC_NVIDIA_DECODER_TIMING_H_
