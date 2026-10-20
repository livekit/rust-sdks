/*
 * Copyright 2026 LiveKit, Inc.
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

#ifndef JETSON_VIDEO_DECODER_FACTORY_H_
#define JETSON_VIDEO_DECODER_FACTORY_H_

#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_decoder_factory.h"

namespace webrtc {

// Hardware decoders on the Jetson NVDEC engine. Set LK_DISABLE_JETSON_DEC to
// fall back to the software decoders.
class JetsonVideoDecoderFactory : public VideoDecoderFactory {
 public:
  JetsonVideoDecoderFactory();
  ~JetsonVideoDecoderFactory() override;

  static bool IsSupported();

  std::vector<SdpVideoFormat> GetSupportedFormats() const override;

  std::unique_ptr<VideoDecoder> Create(const Environment& env,
                                       const SdpVideoFormat& format) override;

 private:
  std::vector<SdpVideoFormat> supported_formats_;
};

}  // namespace webrtc

#endif  // JETSON_VIDEO_DECODER_FACTORY_H_
