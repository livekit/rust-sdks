#include "jetson_video_encoder_factory.h"

#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <optional>
#include <string>
#include <sys/ioctl.h>
#include <unistd.h>
#include <iostream>

#if defined(__linux__)
#include <linux/videodev2.h>
#endif

#include "rtc_base/logging.h"
#include "h264_encoder_impl.h"

namespace webrtc {

namespace {
bool ProbeV4L2EncoderDevice() {
#if defined(__linux__) && (defined(__aarch64__) || defined(__ARM_ARCH))
  // Basic probe: try common video device nodes for M2M encoder capability.
  // Iterate a small set of /dev/video* nodes to find a device with M2M.
  char path[32];
  for (int i = 0; i < 8; ++i) {
    snprintf(path, sizeof(path), "/dev/video%d", i);
    int fd = open(path, O_RDWR | O_NONBLOCK);
    if (fd < 0) {
      continue;
    }
    struct v4l2_capability cap = {};
    if (ioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
      uint32_t dev_caps = (cap.capabilities & V4L2_CAP_DEVICE_CAPS) ? cap.device_caps : cap.capabilities;
      bool is_m2m = (dev_caps & V4L2_CAP_VIDEO_M2M_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_M2M);
      bool has_output = (dev_caps & V4L2_CAP_VIDEO_OUTPUT_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_OUTPUT);
      bool has_capture = (dev_caps & V4L2_CAP_VIDEO_CAPTURE_MPLANE) || (dev_caps & V4L2_CAP_VIDEO_CAPTURE);
      close(fd);
      if (is_m2m && has_output && has_capture) {
        return true;
      }
    } else {
      close(fd);
    }
  }
  return false;
#else
  return false;
#endif
}
}  // namespace

JetsonVideoEncoderFactory::JetsonVideoEncoderFactory() {
  if (IsSupported()) {
    // H.264 Baseline profile (packetization mode 1) as a baseline format
    std::map<std::string, std::string> h264_params = {
        {"profile-level-id", "42e01f"},
        {"level-asymmetry-allowed", "1"},
        {"packetization-mode", "1"},
    };
    supported_formats_.push_back(SdpVideoFormat("H264", h264_params));

    // Advertise H265 if desirable; WebRTC may gate based on SDP support
    supported_formats_.push_back(SdpVideoFormat("H265"));
  }
}

bool JetsonVideoEncoderFactory::IsSupported() {
#if defined(__linux__) && (defined(__aarch64__) || defined(__ARM_ARCH))
  if (!ProbeV4L2EncoderDevice()) {
    RTC_LOG(LS_WARNING) << "JetsonVideoEncoderFactory: no V4L2 M2M encoder found";
    return false;
  }
  RTC_LOG(LS_INFO) << "JetsonVideoEncoderFactory: V4L2 M2M encoder available";
  return true;
#else
  return false;
#endif
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetSupportedFormats() const {
  return supported_formats_;
}

std::unique_ptr<VideoEncoder> JetsonVideoEncoderFactory::Create(
    const Environment& /*env*/,
    const SdpVideoFormat& format) {
  for (const auto& f : supported_formats_) {
    if (format.IsSameCodec(f)) {
      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using Jetson V4L2 H264 encoder";
        return std::make_unique<JetsonH264EncoderImpl>(format);
      }
      // TODO: add H265 implementation
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetImplementations() const {
  return supported_formats_;
}

}  // namespace webrtc


