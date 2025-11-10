/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "jetson_encoder_factory.h"

#include <memory>
#include <string>
#include <sys/stat.h>

#include "rtc_base/logging.h"
#include "h264_encoder_impl.h"

namespace {

bool file_exists(const char* path) {
  struct stat st;
  return stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

bool char_device_exists(const char* path) {
  struct stat st;
  return stat(path, &st) == 0 && S_ISCHR(st.st_mode);
}

}  // namespace

namespace webrtc {

JetsonVideoEncoderFactory::JetsonVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));
}

JetsonVideoEncoderFactory::~JetsonVideoEncoderFactory() {}

bool JetsonVideoEncoderFactory::IsSupported() {
#if defined(__linux__) && (defined(__aarch64__) || defined(__arm__))
  // Heuristics:
  // - Presence of Jetson NV encoder char device
  // - Presence of GStreamer NV V4L2 plugin (commonly installed on Jetson)
  // Either should be sufficient to consider the platform supported.
  // Known device nodes (vary across JetPack versions):
  //   /dev/nvhost-nvenc, /dev/nvhost-nvenc0, /dev/nvhost-nvenc1
  const char* nvenc_nodes[] = {"/dev/nvhost-nvenc", "/dev/nvhost-nvenc0",
                               "/dev/nvhost-nvenc1"};
  for (auto* p : nvenc_nodes) {
    if (char_device_exists(p)) {
      RTC_LOG(LS_INFO) << "Jetson encoder supported (found device " << p << ")";
      return true;
    }
  }

  // GStreamer NV V4L2 plugin path on Jetson (libgstnvvideo4linux2)
  const char* gst_plugins[] = {
      "/usr/lib/aarch64-linux-gnu/gstreamer-1.0/libgstnvvideo4linux2.so",
      "/usr/lib/gstreamer-1.0/libgstnvvideo4linux2.so",
  };
  for (auto* p : gst_plugins) {
    if (file_exists(p)) {
      RTC_LOG(LS_INFO) << "Jetson encoder supported (found " << p << ")";
      return true;
    }
  }

  RTC_LOG(LS_WARNING)
      << "Jetson encoder not detected: no NVENC device nor NV V4L2 plugin.";
  return false;
#else
  return false;
#endif
}

std::unique_ptr<VideoEncoder> JetsonVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      return std::make_unique<JetsonH264EncoderImpl>(env, format);
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc


