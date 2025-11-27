#include "v4l2_encoder_factory.h"

#include <fcntl.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <linux/videodev2.h>
#include <memory>
#include <iostream>
#include <cstring>
#include <cerrno>

#include "v4l2_h264_encoder_impl.h"
#include "v4l2_h265_encoder_impl.h"
#include "rtc_base/logging.h"

namespace webrtc {

V4L2VideoEncoderFactory::V4L2VideoEncoderFactory() {
  device_path_ = GetDevicePath();
  
  if (device_path_.empty()) {
    RTC_LOG(LS_WARNING) << "V4L2 encoder device not found";
    return;
  }

  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  // Advertise HEVC/H265 with default parameters.
  supported_formats_.push_back(SdpVideoFormat("H265"));
  // Some stacks use 'HEVC' name.
  supported_formats_.push_back(SdpVideoFormat("HEVC"));
}

V4L2VideoEncoderFactory::~V4L2VideoEncoderFactory() {}

std::string V4L2VideoEncoderFactory::GetDevicePath() {
  // Try the Jetson-specific device first
  const char* jetson_device = "/dev/v4l2-nvenc";
  int fd = open(jetson_device, O_RDWR);
  if (fd >= 0) {
    // Verify it's a valid V4L2 device that supports QUERYCAP
    struct v4l2_capability cap;
    if (ioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
      close(fd);
      RTC_LOG(LS_INFO) << "Found Jetson encoder device: " << jetson_device;
      return std::string(jetson_device);
    } else {
      RTC_LOG(LS_WARNING) << "Found " << jetson_device << " but QUERYCAP failed (errno: " << errno << " - " << strerror(errno) << "). Skipping.";
    }
    close(fd);
  }

  // Try alternate device paths
  const char* alt_devices[] = {
      "/dev/video0",
      "/dev/video1",
      "/dev/video2",
      "/dev/video3"
  };

  for (const char* device : alt_devices) {
    fd = open(device, O_RDWR);
    if (fd < 0) {
      continue;
    }

    struct v4l2_capability cap;
    if (ioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
      // Check if this is an encoder device
      if ((cap.capabilities & V4L2_CAP_VIDEO_M2M_MPLANE) ||
          (cap.capabilities & V4L2_CAP_VIDEO_M2M)) {
        
        // Try to query supported formats to confirm it's an encoder
        struct v4l2_fmtdesc fmt;
        memset(&fmt, 0, sizeof(fmt));
        fmt.index = 0;
        fmt.type = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
        
        bool is_encoder = false;
        while (ioctl(fd, VIDIOC_ENUM_FMT, &fmt) == 0) {
          if (fmt.pixelformat == V4L2_PIX_FMT_H264 ||
              fmt.pixelformat == V4L2_PIX_FMT_HEVC) {
            is_encoder = true;
            break;
          }
          fmt.index++;
        }
        
        if (is_encoder) {
          close(fd);
          RTC_LOG(LS_INFO) << "Found V4L2 encoder device: " << device 
                          << " (" << cap.card << ")";
          return std::string(device);
        }
      }
    }
    close(fd);
  }

  RTC_LOG(LS_WARNING) << "No suitable V4L2 encoder device found";
  return "";
}

bool V4L2VideoEncoderFactory::IsSupported() {
  std::string device_path = GetDevicePath();
  if (device_path.empty()) {
    RTC_LOG(LS_WARNING) << "V4L2 encoder device not available";
    return false;
  }

  int fd = open(device_path.c_str(), O_RDWR | O_NONBLOCK);
  if (fd < 0) {
    RTC_LOG(LS_WARNING) << "Failed to open V4L2 device: " << device_path 
                        << " (errno: " << errno << " - " << strerror(errno) << ")";
    return false;
  }

  // For Jetson-specific device, just check if we can open it
  if (device_path == "/dev/v4l2-nvenc") {
    close(fd);
    RTC_LOG(LS_INFO) << "V4L2 Encoder is supported on Jetson device: " << device_path;
    return true;
  }

  // For generic V4L2 devices, query capabilities
  struct v4l2_capability cap;
  if (ioctl(fd, VIDIOC_QUERYCAP, &cap) < 0) {
    RTC_LOG(LS_WARNING) << "Failed to query V4L2 capabilities for " << device_path
                        << " (errno: " << errno << " - " << strerror(errno) << ")";
    close(fd);
    // For Jetson, still try to use it even if QUERYCAP fails
    if (device_path.find("nvenc") != std::string::npos) {
      RTC_LOG(LS_INFO) << "Assuming Jetson NVENC device is supported despite QUERYCAP failure";
      return true;
    }
    return false;
  }

  close(fd);

  RTC_LOG(LS_INFO) << "V4L2 Encoder is supported on device: " << device_path 
                   << " (" << cap.card << ")";
  return true;
}

std::unique_ptr<VideoEncoder> V4L2VideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  if (device_path_.empty()) {
    RTC_LOG(LS_ERROR) << "V4L2 encoder device not available";
    return nullptr;
  }

  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using V4L2 HW encoder for H264 (Jetson)";
        return std::make_unique<V4L2H264EncoderImpl>(env, device_path_, format);
      }

      if (format.name == "H265" || format.name == "HEVC") {
        RTC_LOG(LS_INFO) << "Using V4L2 HW encoder for H265/HEVC (Jetson)";
        return std::make_unique<V4L2H265EncoderImpl>(env, device_path_, format);
      }
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> V4L2VideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> V4L2VideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
