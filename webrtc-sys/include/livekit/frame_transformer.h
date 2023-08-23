#pragma once

// #include "rust/cxx.h"
#include "api/frame_transformer_interface.h"
#include "rtc_base/checks.h"
#include "rtc_base/ref_counted_object.h"
#include "rust/cxx.h"
#include <memory>
#include <vector>

namespace livekit {
class EncodedVideoFrame;
class EncodedAudioFrame;
class FrameTransformerInterface;
class AdaptedNativeFrameTransformer;
class AdaptedNativeSenderReportCallback;
class SenderReportInterface;
class SenderReport;
}
#include "webrtc-sys/src/frame_transformer.rs.h"

namespace livekit {

class NativeFrameTransformer : public rtc::RefCountedObject<webrtc::FrameTransformerInterface> {
 public:
  explicit NativeFrameTransformer(rust::Box<EncodedFrameSinkWrapper> observer, bool is_video);

  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> transformable_frame);

 private:
  bool is_video;
  rust::Box<EncodedFrameSinkWrapper> observer_;
};

// from AdaptedVideoTrackSource
class AdaptedNativeFrameTransformer {
 public:
  AdaptedNativeFrameTransformer(rtc::scoped_refptr<NativeFrameTransformer> source);

  rtc::scoped_refptr<NativeFrameTransformer> get() const;

 private:
  rtc::scoped_refptr<NativeFrameTransformer> source_;
};

std::shared_ptr<AdaptedNativeFrameTransformer> new_adapted_frame_transformer(
  rust::Box<EncodedFrameSinkWrapper> observer,
  bool is_video
  );

class NativeSenderReportCallback : public rtc::RefCountedObject<webrtc::SenderReportInterface> {
 public:
  explicit NativeSenderReportCallback(rust::Box<SenderReportSinkWrapper> observer);

  void OnSenderReport(std::unique_ptr<webrtc::LTSenderReport> sender_report);

 private:
  rust::Box<SenderReportSinkWrapper> observer_;
};

class AdaptedNativeSenderReportCallback {
 public:
  AdaptedNativeSenderReportCallback(rtc::scoped_refptr<NativeSenderReportCallback> source);

  rtc::scoped_refptr<NativeSenderReportCallback> get() const;

 private:
  rtc::scoped_refptr<NativeSenderReportCallback> source_;
};

std::shared_ptr<AdaptedNativeSenderReportCallback> new_adapted_sender_report_callback(
  rust::Box<SenderReportSinkWrapper> observer
  );
}  // namespace livekit