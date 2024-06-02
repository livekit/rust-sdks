#include "livekit/objc_video_factory.h"

#import <sdk/objc/components/video_codec/RTCVideoDecoderFactoryH264.h>
#import <sdk/objc/components/video_codec/RTCVideoEncoderFactoryH264.h>

#include "sdk/objc/native/api/video_decoder_factory.h"
#include "sdk/objc/native/api/video_encoder_factory.h"

namespace livekit {

std::unique_ptr<webrtc::VideoEncoderFactory> CreateObjCVideoEncoderFactory() {
  // TODO(theomonnom): Simulcast?
  return webrtc::ObjCToNativeVideoEncoderFactory(
      [[RTCVideoEncoderFactoryH264 alloc] init]);
}

std::unique_ptr<webrtc::VideoDecoderFactory> CreateObjCVideoDecoderFactory() {
  return webrtc::ObjCToNativeVideoDecoderFactory(
      [[RTCVideoDecoderFactoryH264 alloc] init]);
}

}  // namespace livekit
