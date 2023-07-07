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

#include "livekit/video_decoder_factory.h"

#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "livekit/android.h"
#include "livekit/objc_video_factory.h"
#include "media/base/media_constants.h"
#include "rtc_base/logging.h"

namespace livekit {

VideoDecoderFactory::VideoDecoderFactory() {
  factories_.push_back(webrtc::CreateBuiltinVideoDecoderFactory());

#ifdef __APPLE__
  factories_.push_back(livekit::CreateObjCVideoDecoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  factories_.push_back(CreateAndroidVideoDecoderFactory());
#endif

  // TODO(theomonnom): Add other HW decoders here
}

std::vector<webrtc::SdpVideoFormat> VideoDecoderFactory::GetSupportedFormats()
    const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& factory : factories_) {
    auto supported_formats = factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }
  return formats;
}

std::unique_ptr<webrtc::VideoDecoder> VideoDecoderFactory::CreateVideoDecoder(
    const webrtc::SdpVideoFormat& format) {
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format))
        return factory->CreateVideoDecoder(format);
    }
  }

  RTC_LOG(LS_ERROR) << "No VideoDecoder found for " << format.name;
  return nullptr;
}

}  // namespace livekit
