/*
 * Copyright 2023 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/video_encoder_factory.h"

#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "livekit/android.h"
#include "livekit/objc_video_factory.h"
#include "media/base/media_constants.h"
#include "rtc_base/logging.h"

namespace livekit {

VideoEncoderFactory::VideoEncoderFactory() {
  factories_.push_back(webrtc::CreateBuiltinVideoEncoderFactory());

#ifdef __APPLE__
  factories_.push_back(livekit::CreateObjCVideoEncoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  factories_.push_back(CreateAndroidVideoEncoderFactory());
#endif

  // TODO(theomonnom): Add other HW encoders here
}

std::vector<webrtc::SdpVideoFormat> VideoEncoderFactory::GetSupportedFormats()
    const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& factory : factories_) {
    auto supported_formats = factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }
  return formats;
}

std::unique_ptr<webrtc::VideoEncoder> VideoEncoderFactory::CreateVideoEncoder(
    const webrtc::SdpVideoFormat& format) {
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format))
        return factory->CreateVideoEncoder(format);
    }
  }

  RTC_LOG(LS_ERROR) << "No VideoEncoder found for " << format.name;
  return nullptr;
}

}  // namespace livekit
