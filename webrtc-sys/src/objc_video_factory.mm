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

#include "livekit/objc_video_factory.h"

#import <sdk/objc/components/video_codec/RTCVideoDecoderFactoryH264.h>
#import <sdk/objc/components/video_codec/RTCVideoEncoderFactoryH264.h>
#include "sdk/objc/native/api/video_decoder_factory.h"
#include "sdk/objc/native/api/video_encoder_factory.h"

namespace livekit {

std::unique_ptr<webrtc::VideoEncoderFactory> CreateObjCVideoEncoderFactory() {
  // TODO(theomonnom): Simulcast?
  return webrtc::ObjCToNativeVideoEncoderFactory([[RTCVideoEncoderFactoryH264 alloc] init]);
}

std::unique_ptr<webrtc::VideoDecoderFactory> CreateObjCVideoDecoderFactory() {
  return webrtc::ObjCToNativeVideoDecoderFactory([[RTCVideoDecoderFactoryH264 alloc] init]);
}

}  // namespace livekit
