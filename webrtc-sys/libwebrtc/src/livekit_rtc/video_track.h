#ifndef LIVEKIT_RTC_VIDEO_TRACK_H
#define LIVEKIT_RTC_VIDEO_TRACK_H

#include "api/media_stream_interface.h"
#include "api/scoped_refptr.h"
#include "api/video/video_frame.h"
#include "livekit_rtc/capi.h"
#include "livekit_rtc/media_stream_track.h"
#include "livekit_rtc/video_frame.h"
#include "media/base/adapted_video_track_source.h"
#include "rtc_base/synchronization/mutex.h"
#include "rtc_base/timestamp_aligner.h"

namespace livekit {

class NativeVideoSink;

class VideoTrack : public MediaStreamTrack {
 public:
  VideoTrack(webrtc::scoped_refptr<webrtc::VideoTrackInterface> track);
  ~VideoTrack();

  void add_sink(const webrtc::scoped_refptr<NativeVideoSink>& sink) const;
  void remove_sink(const webrtc::scoped_refptr<NativeVideoSink>& sink) const;

  void set_should_receive(bool should_receive) const;
  bool should_receive() const;
  lkContentHint content_hint() const;
  void set_content_hint(lkContentHint hint) const;

 private:
  webrtc::VideoTrackInterface* track() const {
    auto video_track = static_cast<webrtc::VideoTrackInterface*>(MediaStreamTrack::track());
    return video_track;
  }

  mutable webrtc::Mutex mutex_;

  // Same for AudioTrack:
  // Keep a strong reference to the added sinks, so we don't need to
  // manage the lifetime safety on the Rust side
  mutable std::vector<webrtc::scoped_refptr<NativeVideoSink>> sinks_;
};

class NativeVideoSink : public webrtc::VideoSinkInterface<webrtc::VideoFrame>,
                        public webrtc::RefCountInterface {
 public:
  explicit NativeVideoSink(const lkVideoSinkCallabacks* callbacks, void* userdata);

  void OnFrame(const webrtc::VideoFrame& frame) override;
  void OnDiscardedFrame() override;
  void OnConstraintsChanged(
      const webrtc::VideoTrackSourceConstraints& constraints) override;

 private:
  const lkVideoSinkCallabacks* callbacks_;
  void* userdata_;
};

class VideoTrackSource : public webrtc::RefCountInterface {
  class InternalSource : public webrtc::AdaptedVideoTrackSource {
   public:
    InternalSource(const lkVideoResolution&
                       resolution);  // (0, 0) means no resolution/optional, the
                                     // source will guess the resolution at the
                                     // first captured frame
    ~InternalSource() override;

    bool is_screencast() const override;
    std::optional<bool> needs_denoising() const override;
    SourceState state() const override;
    bool remote() const override;
    lkVideoResolution video_resolution() const;
    bool on_captured_frame(const webrtc::VideoFrame& frame);

   private:
    mutable webrtc::Mutex mutex_;
    webrtc::TimestampAligner timestamp_aligner_;
    lkVideoResolution resolution_;
  };

 public:
  VideoTrackSource(const lkVideoResolution& resolution);

  lkVideoResolution video_resolution() const;

  bool on_captured_frame(const webrtc::scoped_refptr<VideoFrame> frame)
      const;  // frames pushed from Rust (+interior mutability)

  webrtc::scoped_refptr<InternalSource> get() const;

  webrtc::VideoTrackSourceInterface* video_source() const { return source_.get(); }

 private:
  webrtc::scoped_refptr<InternalSource> source_;
};

}  // namespace livekit

#endif  // LIVEKIT_RTC_VIDEO_TRACK_H