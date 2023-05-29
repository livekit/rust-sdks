#pragma once

#include "rust/cxx.h"
#include "api/frame_transformer_interface.h"
namespace livekit {
class FrameTransformer;
class FrameTransformerInterface;
}
#include "webrtc-sys/src/frame_transformer.rs.h"

namespace livekit {

class FrameTransformer : public webrtc::FrameTransformerInterface {
  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame);
};

class FrameTransformerInterface {
 public:
  FrameTransformerInterface(rtc::scoped_refptr<FrameTransformer> transformer);

 private:
  rtc::scoped_refptr<FrameTransformer> transformer_;
};

void new_frame_transformer(
    //rust::Box<VideoFrameSinkWrapper> observer
    );

}