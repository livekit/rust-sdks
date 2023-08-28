#pragma once

// #include "rust/cxx.h"
#include "api/frame_transformer_interface.h"
#include "rtc_base/checks.h"
#include "rtc_base/ref_counted_object.h"
#include "rust/cxx.h"
#include <memory>
#include <vector>
#include <thread>
#include <mutex>

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

  void RegisterTransformedFrameCallback(rtc::scoped_refptr<webrtc::TransformedFrameCallback> send_frame_to_sink_callback);
  void UnregisterTransformedFrameCallback();

  void RegisterTransformedFrameSinkCallback(rtc::scoped_refptr<webrtc::TransformedFrameCallback> send_frame_to_sink_callback, uint32_t ssrc);
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc);

  void FrameTransformed(std::unique_ptr<webrtc::TransformableFrameInterface> frame);

 private:
  bool is_video;
  rust::Box<EncodedFrameSinkWrapper> observer_;
  
  mutable std::mutex sink_mutex_;
  rtc::scoped_refptr<webrtc::TransformedFrameCallback> sink_callback_;
  std::map<uint32_t, rtc::scoped_refptr<webrtc::TransformedFrameCallback>> sink_callbacks_;
};

// from AdaptedVideoTrackSource
class AdaptedNativeFrameTransformer {
 public:
  AdaptedNativeFrameTransformer(rtc::scoped_refptr<NativeFrameTransformer> source);

  rtc::scoped_refptr<NativeFrameTransformer> get() const;

  void AudioFrameTransformed(std::unique_ptr<EncodedAudioFrame> frame) const;
  void VideoFrameTransformed(std::unique_ptr<EncodedVideoFrame> frame) const;

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