#ifndef LIVEKIT_ANDROID_H
#define LIVEKIT_ANDROID_H

#include <jni.h>
#include <memory>

#include "api/video_codecs/video_decoder_factory.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace livekit {
void init_android(void* jvm);

std::unique_ptr<webrtc::VideoEncoderFactory> CreateAndroidVideoEncoderFactory();
std::unique_ptr<webrtc::VideoDecoderFactory> CreateAndroidVideoDecoderFactory();

}  // namespace livekit

#endif  // LIVEKIT_ANDROID_H
