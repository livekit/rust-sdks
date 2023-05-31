#pragma once

// #include "rust/cxx.h"
#include "api/frame_transformer_interface.h"
#include "rtc_base/checks.h"
#include "rtc_base/ref_counted_object.h"
#include <memory>
#include <vector>

namespace livekit {
// class NativeFrameTransformer;
class FrameTransformerInterface;
class AdaptedNativeFrameTransformer;
}
#include "webrtc-sys/src/frame_transformer.rs.h"

namespace livekit {

class NativeFrameTransformer : public rtc::RefCountedObject<webrtc::FrameTransformerInterface> {
 public:
  explicit NativeFrameTransformer();

  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame);
  // void RegisterTransformedFrameCallback(
  //     rtc::scoped_refptr<webrtc::TransformedFrameCallback>);
  // void RegisterTransformedFrameSinkCallback(
  //     rtc::scoped_refptr<webrtc::TransformedFrameCallback>,
  //     uint32_t ssrc);
  // void RegisterTransformedFrameSinkCallback(
  //     rtc::scoped_refptr<webrtc::TransformedFrameCallback>,
  //     uint32_t ssrc);
  // void UnregisterTransformedFrameCallback();
  // void UnregisterTransformedFrameSinkCallback(uint32_t ssrc);
};

// from AdaptedVideoTrackSource
class AdaptedNativeFrameTransformer {
 public:
  AdaptedNativeFrameTransformer(rtc::scoped_refptr<NativeFrameTransformer> source);

  rtc::scoped_refptr<NativeFrameTransformer> get() const;

 private:
  rtc::scoped_refptr<NativeFrameTransformer> source_;
};

std::shared_ptr<AdaptedNativeFrameTransformer> new_adapted_frame_transformer();
}