#include "livekit/android.h"

#include <jni.h>

#include <memory>

#include "api/video_codecs/video_decoder_factory.h"
#include "sdk/android/native_api/base/init.h"
#include "sdk/android/native_api/codecs/wrapper.h"
#include "sdk/android/native_api/jni/class_loader.h"
#include "sdk/android/native_api/jni/scoped_java_ref.h"
#include "sdk/android/src/jni/jni_helpers.h"

namespace livekit {

void init_android(void* jvm) {
  webrtc::InitAndroid(jvm);
}

std::unique_ptr<webrtc::VideoEncoderFactory>
CreateAndroidVideoEncoderFactory() {
  JNIEnv* env = webrtc::AttachCurrentThreadIfNeeded();
  webrtc::ScopedJavaLocalRef<jclass> factory_class =
      webrtc::GetClass(env, "org/webrtc/DefaultVideoEncoderFactory");

  jmethodID ctor = env->GetMethodID(factory_class.obj(), "<init>",
                                    "(Lorg/webrtc/EglBase$Context;ZZ)V");

  jobject encoder_factory =
      env->NewObject(factory_class.obj(), ctor, nullptr, true, false);

  return webrtc::JavaToNativeVideoEncoderFactory(env, encoder_factory);
}

std::unique_ptr<webrtc::VideoDecoderFactory>
CreateAndroidVideoDecoderFactory() {
  JNIEnv* env = webrtc::AttachCurrentThreadIfNeeded();

  webrtc::ScopedJavaLocalRef<jclass> factory_class =
      webrtc::GetClass(env, "org/webrtc/WrappedVideoDecoderFactory");

  jmethodID ctor = env->GetMethodID(factory_class.obj(), "<init>",
                                    "(Lorg/webrtc/EglBase$Context;)V");

  jobject decoder_factory = env->NewObject(factory_class.obj(), ctor, nullptr);
  return webrtc::JavaToNativeVideoDecoderFactory(env, decoder_factory);
}

}  // namespace livekit
